use proc_macro2::TokenStream;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;

use crate::diagnostic_shim::*;

pub fn expand(item: syn::ItemFn) -> Result<TokenStream, Diagnostic> {
    let job = BackgroundJob::try_from(item)?;

    let attrs = job.attrs;
    let vis = job.visibility;
    let fn_token = job.fn_token;
    let name = job.name;
    let env_pat = &job.args.env_arg.pat;
    let env_type = &job.args.env_arg.ty;
    let connection_arg = &job.args.connection_arg;
    let pool_pat = connection_arg.pool_pat();
    let fn_args = job.args.iter();
    let struct_def = job.args.struct_def();
    let struct_assign = job.args.struct_assign();
    let arg_names = job.args.names();
    let return_type = job.return_type;
    let body = connection_arg.wrap(job.body);

    let res = quote! {
        #(#attrs)*
        #vis #fn_token #name (#(#fn_args),*) -> #name :: Job {
            #name :: Job {
                #(#struct_assign),*
            }
        }

        #[swirl::async_trait::async_trait]
        impl swirl::Job for #name :: Job {
            type Environment = #env_type;
            const JOB_TYPE: &'static str = stringify!(#name);

            async #fn_token perform(self, #env_pat: &Self::Environment, #pool_pat: swirl::DieselPool) #return_type {
                let Self { #(#arg_names),* } = self;
                #body
            }
        }

        mod #name {
            use super::*;

            #[derive(swirl::Serialize, swirl::Deserialize)]
            #[serde(crate = "swirl::serde")]
            pub struct Job {
                #(#struct_def),*
            }

            swirl::register_job!(Job);
        }
    };

    Ok(res)
}

struct BackgroundJob {
    attrs: Vec<syn::Attribute>,
    visibility: syn::Visibility,
    fn_token: syn::Token![fn],
    name: syn::Ident,
    args: JobArgs,
    return_type: syn::ReturnType,
    body: Vec<syn::Stmt>,
}

impl BackgroundJob {
    fn try_from(item: syn::ItemFn) -> Result<Self, Diagnostic> {
        let syn::ItemFn {
            attrs,
            vis,
            sig,
            block,
        } = item;

        if let Some(constness) = sig.constness {
            return Err(constness
                .span
                .error("#[swirl::background_job] cannot be used on const functions"));
        }

        if let Some(unsafety) = sig.unsafety {
            return Err(unsafety
                .span
                .error("#[swirl::background_job] cannot be used on unsafe functions"));
        }

        if let Some(abi) = sig.abi {
            return Err(abi
                .span()
                .error("#[swirl::background_job] cannot be used on functions with an abi"));
        }

        if !sig.generics.params.is_empty() {
            return Err(sig
                .generics
                .span()
                .error("#[swirl::background_job] cannot be used on generic functions"));
        }

        if let Some(where_clause) = sig.generics.where_clause {
            return Err(where_clause.where_token.span.error(
                "#[swirl::background_job] cannot be used on functions with a where clause",
            ));
        }

        let fn_token = sig.fn_token;
        let return_type = sig.output.clone();
        let ident = sig.ident.clone();
        let job_args = JobArgs::try_from(sig)?;

        Ok(Self {
            attrs,
            visibility: vis,
            fn_token,
            name: ident,
            args: job_args,
            return_type,
            body: block.stmts,
        })
    }
}

struct JobArgs {
    env_arg: EnvArg,
    connection_arg: ConnectionArg,
    args: Punctuated<syn::PatType, syn::Token![,]>,
}

impl JobArgs {
    fn iter(&self) -> <&Self as IntoIterator>::IntoIter {
        self.into_iter()
    }

    fn try_from(decl: syn::Signature) -> Result<Self, Diagnostic> {
        let mut env_arg = None;
        let mut connection_arg = ConnectionArg::None;
        let mut args = Punctuated::new();

        for fn_arg in decl.inputs {
            let pat_type = match fn_arg {
                syn::FnArg::Receiver(..) => {
                    return Err(fn_arg.span().error("Background jobs cannot take self"));
                }
                syn::FnArg::Typed(pat_type) => pat_type,
            };

            if let syn::Pat::Ident(syn::PatIdent {
                by_ref: None,
                subpat: None,
                ..
            }) = *pat_type.pat
            {
                // ok
            } else {
                return Err(pat_type
                    .pat
                    .span()
                    .error("#[swirl::background_job] cannot yet handle patterns"));
            }

            let span = pat_type.span();
            match (&env_arg, &connection_arg, Arg::try_from(pat_type)?) {
                (None, _, Arg::Env(arg)) => env_arg = Some(arg),
                (Some(_), _, Arg::Env(_)) => {
                    return Err(
                        span.error("Background job functions cannot take references as arguments")
                    );
                }
                (_, ConnectionArg::None, Arg::Connection(arg)) => connection_arg = arg,
                (_, _, Arg::Connection(_)) => {
                    return Err(
                        span.error("Multiple database connection arguments in job function")
                            .help("To take a connection pool as an argument, define an argument of the type`swirl::DieselPool`")
                    );
                }
                (_, _, Arg::Normal(pat_type)) => args.push(pat_type),
            }
        }

        Ok(Self {
            env_arg: env_arg.unwrap_or_default(),
            connection_arg,
            args,
        })
    }

    fn struct_def(&self) -> impl Iterator<Item = proc_macro2::TokenStream> + '_ {
        self.args.iter().map(|arg| quote::quote!(pub(super) #arg))
    }

    fn struct_assign(&self) -> impl Iterator<Item = syn::FieldValue> + '_ {
        self.names().map(|ident| syn::parse_quote!(#ident: #ident))
    }

    fn names(&self) -> impl Iterator<Item = syn::Ident> + '_ {
        self.args.iter().map(|arg| match &*arg.pat {
            syn::Pat::Ident(pat_ident) => pat_ident.ident.clone(),
            _ => unreachable!(),
        })
    }
}

impl<'a> IntoIterator for &'a JobArgs {
    type Item = <&'a Punctuated<syn::PatType, syn::Token![,]> as IntoIterator>::Item;
    type IntoIter = <&'a Punctuated<syn::PatType, syn::Token![,]> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        (&self.args).into_iter()
    }
}

enum Arg {
    Env(EnvArg),
    Connection(ConnectionArg),
    Normal(syn::PatType),
}

impl Arg {
    fn try_from(pat_type: syn::PatType) -> Result<Self, Diagnostic> {
        if let syn::Type::Reference(type_ref) = *pat_type.ty {
            if let Some(mutable) = type_ref.mutability {
                return Err(mutable.span.error("Unexpected `mut`"));
            }
            let pat = pat_type.pat;
            let ty = type_ref.elem;
            // This can only be an Env argument.
            Ok(Arg::Env(EnvArg { pat, ty }))
        } else if let syn::Type::Path(type_path) = pat_type.ty.as_ref() {
            if ConnectionArg::is_connection_arg(type_path) {
                if let syn::Pat::Ident(pool_ident) = *pat_type.pat {
                    Ok(Arg::Connection(ConnectionArg::Pool(pool_ident.ident)))
                } else {
                    Err(pat_type
                        .span()
                        .error("Expected an identifier for the DB connection parameter"))
                }
            } else {
                Ok(Arg::Normal(pat_type))
            }
        } else {
            Ok(Arg::Normal(pat_type))
        }
    }
}

struct EnvArg {
    pat: Box<syn::Pat>,
    ty: Box<syn::Type>,
}

impl Default for EnvArg {
    fn default() -> Self {
        Self {
            pat: syn::parse_quote!(_),
            ty: syn::parse_quote!(()),
        }
    }
}

enum ConnectionArg {
    None,
    Pool(syn::Ident),
}

impl ConnectionArg {
    fn is_connection_arg(tp: &syn::TypePath) -> bool {
        path_ends_with(&tp.path, "DieselPool")
    }

    fn pool_pat(&self) -> syn::Pat {
        match self {
            ConnectionArg::None => {
                syn::parse_quote!(_)
            }
            ConnectionArg::Pool(arg_name) => {
                syn::parse_quote!(#arg_name)
            }
        }
    }

    fn wrap(&self, body: Vec<syn::Stmt>) -> TokenStream {
        quote!(#(#body)*)
    }
}

fn path_ends_with(path: &syn::Path, needle: &str) -> bool {
    path.segments
        .last()
        .map(|s| s.arguments.is_empty() && s.ident == needle)
        .unwrap_or(false)
}
