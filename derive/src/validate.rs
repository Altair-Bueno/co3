use std::collections::BTreeSet;

use syn::{Error, Result, Type, visit::Visit};

use crate::{
    dispatch::HandleId,
    ffi_fn::is_by_val_attr,
    find_dispatch_attr, is_explicit_lifetimes_attr, is_symbol_name_attr,
    parse::ParsedForeignItem,
    trait_object_single_trait_bound,
    utils::{has_non_lifetime_generics, is_drop_impl, is_type_erased, push_error},
};

const NO_PREDICATE_ERR: &str = "Type parameters require a tagged-dispatch predicate";
const NO_GENERICS_ERR: &str = "tagged dispatch requires at least one `dyn Type` or `dyn Self`";
const DYN_SELF_DECLARED_TYPE_ERR: &str = "`dyn Self` is only supported for declared types";
const DYN_SELF_GENERIC_SELF_ERR: &str = "`dyn Self` is only supported on generic `Self`";
const DISPATCH_WRAPPER_ERR: &str = "tagged-dispatch parameters cannot be used inside wrapper types";

fn is_type_param_path(ty: &syn::TypePath, type_params: &BTreeSet<&syn::Ident>) -> bool {
    ty.qself.is_none()
        && ty
            .path
            .get_ident()
            .is_some_and(|ident| type_params.contains(ident))
}

struct TypeParamUseVisitor<'params, 'ident> {
    type_params: &'params BTreeSet<&'ident syn::Ident>,
    found: bool,
}

impl Visit<'_> for TypeParamUseVisitor<'_, '_> {
    fn visit_type_path(&mut self, ty: &syn::TypePath) {
        if is_type_param_path(ty, self.type_params) {
            self.found = true;
            return;
        }

        syn::visit::visit_type_path(self, ty);
    }
}

pub(crate) fn validate_niche_value_sized_tail(fields: &syn::Fields) -> Result<()> {
    fn peel_type(ty: &syn::Type) -> &syn::Type {
        match ty {
            syn::Type::Group(ty) => peel_type(&ty.elem),
            syn::Type::Paren(ty) => peel_type(&ty.elem),
            _ => ty,
        }
    }

    fn is_always_unsized(ty: &syn::Type) -> bool {
        match peel_type(ty) {
            syn::Type::Slice(_) | syn::Type::TraitObject(_) => true,
            syn::Type::Path(ty) if ty.qself.is_none() => ty.path.is_ident("str"),
            _ => false,
        }
    }

    let Some(field) = fields.iter().next_back() else {
        return Ok(());
    };

    if !is_always_unsized(&field.ty) {
        return Ok(());
    }

    let err_msg = "`NICHE_VALUE` is not supported on unsized types";
    Err(Error::new_spanned(&field.ty, err_msg))
}

fn unsupported_attr(attr: &syn::Attribute) -> Error {
    Error::new_spanned(attr, "Attribute not supported in this position")
}

fn is_cfg_attr(attr: &syn::Attribute) -> bool {
    attr.path().is_ident("cfg") || attr.path().is_ident("cfg_attr")
}

fn handle_id<'a>(ty: &'a syn::Type, self_ty: Option<&syn::Type>) -> Option<HandleId<'a>> {
    if let Type::Path(syn::TypePath { path, qself }) = ty
        && path.segments.len() == 1
        && path.segments.first().unwrap().ident == "ID"
        && self_ty.is_some_and(|self_ty| {
            qself.as_ref().is_some_and(|syn::QSelf { ty, .. }| {
                matches!(&**ty, Type::TraitObject(_)) && &**ty == self_ty
            })
        })
    {
        return Some(HandleId::DynSelf);
    }

    crate::dispatch::handle_id(ty)
}

fn validate_export_fn_attrs(attrs: &[syn::Attribute]) -> Result<()> {
    for attr in attrs {
        if attr.path().is_ident("erased") {
            continue;
        }
        if is_symbol_name_attr(attr)
            || is_explicit_lifetimes_attr(attr)
            || is_by_val_attr(attr)
            || is_cfg_attr(attr)
        {
            continue;
        }

        return Err(unsupported_attr(attr));
    }

    Ok(())
}

fn has_explicit_lifetimes_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(is_explicit_lifetimes_attr)
}

fn validate_lifetime_opt_in(attrs: &[syn::Attribute], generics: &syn::Generics) -> Result<()> {
    fn param_has_explicit_lifetime(param: &syn::GenericParam) -> bool {
        let syn::GenericParam::Lifetime(param) = param else {
            return false;
        };

        param.lifetime.ident != "_"
    }

    if !generics.params.iter().any(param_has_explicit_lifetime) {
        return Ok(());
    }

    if has_explicit_lifetimes_attr(attrs) {
        return Ok(());
    }

    let err_msg = "Explicit lifetimes found but no `#[explicit_lifetimes]`";
    Err(Error::new_spanned(generics, err_msg))
}

fn validate_no_dispatch_attrs(attrs: &[syn::Attribute], errors: &mut Option<Error>) {
    for attr in attrs {
        if attr.path().is_ident("erased") {
            let err_msg = "tagged-dispatch predicates are only supported on impl blocks";
            push_error(errors, Error::new_spanned(attr, err_msg));
        }
    }
}

fn ensure_no_handle_arg_attrs(sig: &syn::Signature) -> Result<()> {
    fn validate_no_lifetimes_attrs(attrs: &[syn::Attribute], errors: &mut Option<Error>) {
        for attr in attrs {
            if is_explicit_lifetimes_attr(attr) {
                push_error(errors, unsupported_attr(attr));
            }
        }
    }

    let mut errors = None;
    for input in &sig.inputs {
        match input {
            syn::FnArg::Receiver(receiver) => {
                validate_no_lifetimes_attrs(&receiver.attrs, &mut errors);
                validate_no_dispatch_attrs(&receiver.attrs, &mut errors);
            }
            syn::FnArg::Typed(arg) => {
                validate_no_lifetimes_attrs(&arg.attrs, &mut errors);
                validate_no_dispatch_attrs(&arg.attrs, &mut errors);
            }
        }
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(())
}

pub(crate) fn validate_export_decls(decls: &[ParsedForeignItem]) -> Result<()> {
    validate_decls(decls, validate_export_decl)
}

pub(crate) fn validate_extern_decls(decls: &[ParsedForeignItem]) -> Result<()> {
    validate_decls(decls, validate_extern_decl)
}

fn validate_decls(
    decls: &[ParsedForeignItem],
    validate_decl: impl Fn(&ParsedForeignItem) -> Result<()>,
) -> Result<()> {
    let mut errors = None;

    if let Err(err) = validate_shared(decls) {
        push_error(&mut errors, err);
    }

    for decl in decls {
        if let Err(err) = validate_decl(decl) {
            push_error(&mut errors, err);
        }
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(())
}

pub(crate) fn validate_export_attrs(attrs: &[syn::Attribute]) -> Result<()> {
    for attr in attrs {
        if !attr.path().is_ident("feature") && !is_cfg_attr(attr) {
            return Err(unsupported_attr(attr));
        }
    }

    Ok(())
}

fn validate_export_decl(decl: &ParsedForeignItem) -> Result<()> {
    let mut errors = None;

    match decl {
        ParsedForeignItem::Static(static_) => {
            for attr in &static_.attrs {
                if !is_symbol_name_attr(attr) && !is_cfg_attr(attr) {
                    push_error(&mut errors, unsupported_attr(attr));
                }
            }
            if static_.expr.is_none() {
                let err_msg = "export static declarations require an initializer";
                push_error(&mut errors, Error::new_spanned(&static_.ident, err_msg));
            }
        }
        ParsedForeignItem::Impl(impl_) => {
            for item in &impl_.items {
                let syn::ImplItem::Fn(method) = item else {
                    continue;
                };

                if let Err(err) = validate_export_fn_attrs(&method.attrs) {
                    push_error(&mut errors, err);
                }
                if let Err(err) = reject_explicit_dispatch_ids(&method.sig, &impl_.self_ty) {
                    push_error(&mut errors, err);
                }
            }
        }
        ParsedForeignItem::Fn(decl_fn) => {
            if let Err(err) = validate_export_fn_attrs(&decl_fn.attrs) {
                push_error(&mut errors, err);
            }
        }
        ParsedForeignItem::Type(decl) => {
            for attr in &decl.ty.attrs {
                if !attr.path().is_ident("id")
                    && !attr.path().is_ident("erased")
                    && !is_cfg_attr(attr)
                {
                    push_error(&mut errors, unsupported_attr(attr));
                }
            }
        }
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(())
}

fn validate_extern_decl(decl: &ParsedForeignItem) -> Result<()> {
    let mut errors = None;

    match decl {
        ParsedForeignItem::Static(static_) => {
            for attr in &static_.attrs {
                if !is_symbol_name_attr(attr) && !is_cfg_attr(attr) {
                    push_error(&mut errors, unsupported_attr(attr));
                }
            }
            if static_.expr.is_some() {
                let err_msg = "extern static declarations cannot have an initializer";
                push_error(&mut errors, Error::new_spanned(&static_.ident, err_msg));
            }
        }
        ParsedForeignItem::Fn(item) if find_dispatch_attr(&item.attrs).is_some() => {
            let dispatch_params = item
                .sig
                .generics
                .type_params()
                .filter(|param| param.attrs.iter().any(is_type_erased));

            if let Err(err) = validate_extern_dispatch_sig(dispatch_params, None, &item.sig) {
                push_error(&mut errors, err);
            }
        }
        ParsedForeignItem::Impl(impl_) => {
            let impl_dispatch = find_dispatch_attr(&impl_.attrs).is_some();

            for item in &impl_.items {
                let syn::ImplItem::Fn(method) = item else {
                    continue;
                };

                let method_dispatch = find_dispatch_attr(&method.attrs).is_some();
                if !impl_dispatch && !method_dispatch {
                    continue;
                }

                let dispatch_params =
                    impl_
                        .generics
                        .type_params()
                        .filter(|param| impl_dispatch && param.attrs.iter().any(is_type_erased))
                        .chain(method.sig.generics.type_params().filter(|param| {
                            method_dispatch && param.attrs.iter().any(is_type_erased)
                        }));

                if let Err(err) =
                    validate_extern_dispatch_sig(dispatch_params, Some(&impl_.self_ty), &method.sig)
                {
                    push_error(&mut errors, err);
                }
            }
        }
        _ => {}
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(())
}

fn validate_shared(decls: &[ParsedForeignItem]) -> Result<()> {
    let mut errors = None;

    for decl in decls {
        match decl {
            ParsedForeignItem::Static(static_) => {
                validate_no_dispatch_attrs(&static_.attrs, &mut errors);
            }
            ParsedForeignItem::Type(decl) => {
                for attr in &decl.ty.attrs {
                    if is_explicit_lifetimes_attr(attr) {
                        push_error(&mut errors, unsupported_attr(attr));
                    }
                }
                validate_no_dispatch_attrs(&decl.ty.attrs, &mut errors);
            }
            ParsedForeignItem::Fn(decl_fn) => {
                let is_dispatch = find_dispatch_attr(&decl_fn.attrs).is_some();

                if let Err(err) = ensure_no_handle_arg_attrs(&decl_fn.sig) {
                    push_error(&mut errors, err);
                }
                if let Err(err) = validate_fn_generics(&decl_fn.sig.generics, is_dispatch) {
                    push_error(&mut errors, err);
                }
                if let Err(err) = validate_signature_shape(None, &decl_fn.sig) {
                    push_error(&mut errors, err);
                }
                if is_dispatch {
                    let dispatch_params = decl_fn.sig.generics.type_params().filter(|param| {
                        param.attrs.iter().any(is_type_erased) && param.default.is_some()
                    });

                    if let Err(err) =
                        validate_dispatch_param_positions(dispatch_params, &decl_fn.sig)
                    {
                        push_error(&mut errors, err);
                    }
                }
                if let Err(err) = validate_lifetime_opt_in(&decl_fn.attrs, &decl_fn.sig.generics) {
                    push_error(&mut errors, err);
                }
            }
            ParsedForeignItem::Impl(impl_) => {
                let self_ty = &impl_.self_ty;

                for attr in &impl_.attrs {
                    if !attr.path().is_ident("erased")
                        && !is_explicit_lifetimes_attr(attr)
                        && !is_cfg_attr(attr)
                    {
                        push_error(&mut errors, unsupported_attr(attr));
                    }
                }

                let is_dispatch_impl = find_dispatch_attr(&impl_.attrs).is_some();
                if !is_dispatch_impl && has_non_lifetime_generics(&impl_.generics) {
                    let err = Error::new_spanned(&impl_.generics, NO_PREDICATE_ERR);
                    push_error(&mut errors, err);
                }

                if is_dispatch_impl {
                    if let Err(err) = validate_dispatch_form(&impl_.generics, Some(self_ty)) {
                        push_error(&mut errors, err);
                    }

                    if let Err(err) = validate_dispatched_self_ty(&impl_.generics, self_ty) {
                        push_error(&mut errors, err);
                    }
                }

                if let Err(err) = validate_lifetime_opt_in(&impl_.attrs, &impl_.generics) {
                    push_error(&mut errors, err);
                }

                for item in &impl_.items {
                    if let syn::ImplItem::Fn(syn::ImplItemFn { attrs, sig, .. }) = item {
                        let dispatch_attr = find_dispatch_attr(attrs);

                        if let Err(err) = ensure_no_handle_arg_attrs(sig) {
                            push_error(&mut errors, err);
                        }

                        let supports_method_dispatch = impl_.trait_.is_none();
                        if let Some(dispatch_attr) = dispatch_attr
                            && !supports_method_dispatch
                        {
                            let err_msg = "tagged dispatch is not supported on trait methods";
                            let err = Error::new_spanned(dispatch_attr, err_msg);

                            push_error(&mut errors, err);
                        } else if let Err(err) =
                            validate_fn_generics(&sig.generics, dispatch_attr.is_some())
                        {
                            push_error(&mut errors, err);
                        }

                        if let Err(err) = validate_signature_shape(Some(self_ty), sig) {
                            push_error(&mut errors, err);
                        }
                        if is_dispatch_impl || dispatch_attr.is_some() {
                            let dispatch_params = impl_
                                .generics
                                .type_params()
                                .filter(|param| {
                                    is_dispatch_impl
                                        && param.attrs.iter().any(is_type_erased)
                                        && param.default.is_some()
                                })
                                .chain(sig.generics.type_params().filter(|param| {
                                    dispatch_attr.is_some()
                                        && param.attrs.iter().any(is_type_erased)
                                        && param.default.is_some()
                                }));
                            if let Err(err) =
                                validate_dispatch_param_positions(dispatch_params, sig)
                            {
                                push_error(&mut errors, err);
                            }
                        }
                        if let Err(err) = validate_lifetime_opt_in(attrs, &sig.generics) {
                            push_error(&mut errors, err);
                        }
                    }
                }

                if is_drop_impl(impl_)
                    && let Err(err) = validate_drop_impl(impl_)
                {
                    push_error(&mut errors, err);
                }
            }
        }
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(())
}

fn validate_dispatch_form(generics: &syn::Generics, self_ty: Option<&syn::Type>) -> Result<()> {
    if generics
        .type_params()
        .any(|param| param.attrs.iter().any(is_type_erased))
    {
        return Ok(());
    }

    if self_ty.is_some_and(|self_ty| matches!(self_ty, syn::Type::TraitObject(_))) {
        return Ok(());
    }

    match self_ty {
        Some(self_ty) => Err(Error::new_spanned(self_ty, NO_GENERICS_ERR)),
        None => Err(Error::new_spanned(generics, NO_GENERICS_ERR)),
    }
}

fn validate_fn_generics(generics: &syn::Generics, is_dispatch: bool) -> Result<()> {
    let mut errors = None;

    if !is_dispatch && has_non_lifetime_generics(generics) {
        return Err(Error::new_spanned(generics, NO_PREDICATE_ERR));
    }

    if !is_dispatch {
        return Ok(());
    }

    for param in &generics.params {
        match param {
            syn::GenericParam::Lifetime(_) => {}
            syn::GenericParam::Type(param) if param.attrs.iter().any(is_type_erased) => {}
            syn::GenericParam::Type(_) | syn::GenericParam::Const(_) => {
                let err_msg = "tagged dispatch requires every generic param to be `dyn(TagTy)`";
                push_error(&mut errors, Error::new_spanned(param, err_msg));
            }
        }
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    validate_dispatch_form(generics, None)
}

fn reject_explicit_dispatch_ids(sig: &syn::Signature, self_ty: &syn::Type) -> Result<()> {
    let err_msg = "explicit `<dyn Type>::ID` is only supported in extern declarations";

    let mut errors = None;
    for input in &sig.inputs {
        let syn::FnArg::Typed(arg) = input else {
            continue;
        };

        if handle_id(&arg.ty, Some(self_ty)).is_some() {
            push_error(&mut errors, Error::new_spanned(&arg.ty, err_msg));
        }
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(())
}

fn validate_extern_dispatch_sig<'a>(
    dispatch_params: impl Iterator<Item = &'a syn::TypeParam>,
    self_ty: Option<&syn::Type>,
    sig: &syn::Signature,
) -> Result<()> {
    let mut handle_ids = BTreeSet::new();

    let handle_tys = dispatch_params
        .map(|param| &param.ident)
        .collect::<BTreeSet<_>>();

    let mut errors = None;
    for input in &sig.inputs {
        let syn::FnArg::Typed(arg) = input else {
            continue;
        };

        let Some(handle_id) = handle_id(&arg.ty, self_ty) else {
            continue;
        };

        let is_dyn_param = match handle_id {
            HandleId::DynType(ident) => handle_tys.contains(ident),
            HandleId::DynSelf => true,
        };

        if !is_dyn_param {
            let err_msg = "`<dyn Type>::ID` arg must target declared `dyn Type`";
            push_error(&mut errors, Error::new_spanned(&arg.ty, err_msg));
            continue;
        }

        if !handle_ids.insert(handle_id) {
            let err_msg = "duplicate `<dyn Type>::ID`";
            push_error(&mut errors, Error::new_spanned(&arg.ty, err_msg));
        }
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(())
}

fn validate_dispatch_param_positions<'a>(
    dispatch_params: impl Iterator<Item = &'a syn::TypeParam>,
    sig: &syn::Signature,
) -> Result<()> {
    struct DispatchParamPositionValidator<'a> {
        params: BTreeSet<&'a syn::Ident>,
        errors: Option<Error>,
    }

    impl Visit<'_> for DispatchParamPositionValidator<'_> {
        fn visit_type_path(&mut self, ty: &syn::TypePath) {
            if is_type_param_path(ty, &self.params) {
                return syn::visit::visit_type_path(self, ty);
            }

            let first_seg = ty.path.segments.first();
            let is_associated_type_of_param = first_seg
                .is_some_and(|seg| self.params.contains(&seg.ident))
                || ty.qself.as_ref().is_some_and(|qself| {
                    let mut visitor = TypeParamUseVisitor {
                        type_params: &self.params,
                        found: false,
                    };
                    visitor.visit_type(&qself.ty);
                    visitor.found
                });
            if is_associated_type_of_param {
                let err = Error::new_spanned(ty, DISPATCH_WRAPPER_ERR);
                push_error(&mut self.errors, err);
                return;
            }

            let mut visitor = TypeParamUseVisitor {
                type_params: &self.params,
                found: false,
            };
            visitor.visit_type_path(ty);
            if visitor.found {
                let err = Error::new_spanned(ty, DISPATCH_WRAPPER_ERR);
                push_error(&mut self.errors, err);
                return;
            }

            syn::visit::visit_type_path(self, ty);
        }
    }

    let mut validator = DispatchParamPositionValidator {
        params: dispatch_params.map(|param| &param.ident).collect(),
        errors: None,
    };

    for input in &sig.inputs {
        let ty = match input {
            syn::FnArg::Receiver(receiver) => &receiver.ty,
            syn::FnArg::Typed(arg) => &arg.ty,
        };

        validator.visit_type(ty);
    }

    if let syn::ReturnType::Type(_, ty) = &sig.output {
        validator.visit_type(ty);
    }

    validator.errors.map_or(Ok(()), Err)
}

fn validate_drop_impl(impl_: &syn::ItemImpl) -> Result<()> {
    const UNKNOWN_METHOD: &str = "`Drop` must have exactly one method `drop`";

    fn is_mut_self_ty(ty: &Type) -> bool {
        matches!(ty, Type::Reference(reference) if reference.mutability.is_some())
    }

    let mut items = impl_.items.iter();
    let Some(item) = items.next() else {
        return Err(Error::new_spanned(impl_, UNKNOWN_METHOD));
    };
    if let Some(item) = items.next() {
        return Err(Error::new_spanned(item, UNKNOWN_METHOD));
    }
    let syn::ImplItem::Fn(method) = item else {
        return Err(Error::new_spanned(item, UNKNOWN_METHOD));
    };
    if method.sig.ident != "drop" {
        return Err(Error::new_spanned(&method.sig.ident, UNKNOWN_METHOD));
    }
    if !matches!(method.sig.output, syn::ReturnType::Default) {
        let err_msg = "`Drop::drop` must have no return type";
        return Err(Error::new_spanned(&method.sig.output, err_msg));
    }

    let mut was_receiver = false;
    for input in &method.sig.inputs {
        match input {
            syn::FnArg::Typed(arg) if handle_id(&arg.ty, Some(&impl_.self_ty)).is_some() => {}
            syn::FnArg::Receiver(receiver) if is_mut_self_ty(receiver.ty.as_ref()) => {
                if was_receiver {
                    let err_msg = "`Drop::drop` can have only one receiver argument `&mut self`";
                    return Err(Error::new_spanned(&method.sig.inputs, err_msg));
                }

                was_receiver = true;
            }
            _ => {
                let err_msg = "`Drop::drop` supports only `&mut self` and optionally a handle ID";
                return Err(Error::new_spanned(&method.sig.inputs, err_msg));
            }
        }
    }

    Ok(())
}

fn validate_signature_shape(self_ty: Option<&syn::Type>, sig: &syn::Signature) -> Result<()> {
    if let Some(asyncness) = sig.asyncness {
        let err_msg = "Async functions are not supported";
        return Err(Error::new_spanned(asyncness, err_msg));
    }
    if let Some(variadic) = &sig.variadic {
        let err_msg = "Variadic arguments are not supported";
        return Err(Error::new_spanned(variadic, err_msg));
    }
    for input in &sig.inputs {
        let syn::FnArg::Typed(arg) = input else {
            continue;
        };

        validate_pat_type_shape(arg)?;
        if let Some(self_ty) = self_ty {
            validate_handle_id_pos(&arg.ty, self_ty)?;
        }
    }

    if let syn::ReturnType::Type(_, output) = &sig.output
        && let Some(self_ty) = self_ty
    {
        if handle_id(output, Some(self_ty)).is_some() {
            let err_msg = "`<dyn Type>::ID` is not allowed in return position";
            return Err(Error::new_spanned(output, err_msg));
        }

        validate_handle_id_pos(output, self_ty)?;
    }

    Ok(())
}

fn validate_pat_type_shape(arg: &syn::PatType) -> Result<()> {
    let err_msg = "patterns aren't allowed in function declarations";

    match arg.pat.as_ref() {
        syn::Pat::Ident(ident) => {
            if ident.by_ref.is_some() && ident.mutability.is_some() && ident.subpat.is_some() {
                return Err(Error::new_spanned(ident, err_msg));
            }

            Ok(())
        }
        _ => Err(Error::new_spanned(&arg.pat, err_msg)),
    }
}

fn validate_handle_id_pos(ty: &Type, self_ty: &syn::Type) -> Result<()> {
    struct NestedHandleIdVisitor<'a> {
        errors: Option<Error>,
        self_ty: &'a syn::Type,
        depth: usize,
    }

    impl Visit<'_> for NestedHandleIdVisitor<'_> {
        fn visit_type(&mut self, node: &Type) {
            if self.depth != 0 && handle_id(node, Some(self.self_ty)).is_some() {
                let err_msg = "`<dyn Type>::ID` is only allowed as a top-level function argument";
                push_error(&mut self.errors, Error::new_spanned(node, err_msg));
                return;
            }

            self.depth += 1;
            syn::visit::visit_type(self, node);
            self.depth -= 1;
        }
    }

    let mut visitor = NestedHandleIdVisitor {
        errors: None,
        depth: 0,
        self_ty,
    };

    visitor.visit_type(ty);
    if let Some(errors) = visitor.errors {
        return Err(errors);
    }

    Ok(())
}

fn validate_dispatched_self_ty(generics: &syn::Generics, self_ty: &syn::Type) -> Result<()> {
    let Some(trait_bound) = trait_object_single_trait_bound(self_ty) else {
        return Ok(());
    };

    let type_params = generics
        .type_params()
        .map(|param| &param.ident)
        .collect::<BTreeSet<_>>();

    if trait_bound.path.segments.len() > 1 {
        return Err(Error::new_spanned(self_ty, DYN_SELF_DECLARED_TYPE_ERR));
    }

    if let Some(ident) = trait_bound.path.get_ident()
        && type_params.contains(ident)
    {
        return Err(Error::new_spanned(self_ty, DYN_SELF_DECLARED_TYPE_ERR));
    }

    let mut visitor = TypeParamUseVisitor {
        type_params: &type_params,
        found: false,
    };
    visitor.visit_trait_bound(trait_bound);

    if visitor.found {
        return Ok(());
    }

    Err(Error::new_spanned(self_ty, DYN_SELF_GENERIC_SELF_ERR))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_dispatched_self_without_type_param_in_generic_args() {
        let generics: syn::Generics = syn::parse_quote!(<T>);
        let self_ty: syn::Type = syn::parse_quote!(dyn Opaque<u8>);

        let err = validate_dispatched_self_ty(&generics, &self_ty).unwrap_err();
        assert!(err.to_string().contains(DYN_SELF_GENERIC_SELF_ERR));
    }

    #[test]
    fn accepts_dispatched_self_with_type_param_in_generic_args() {
        let generics: syn::Generics = syn::parse_quote!(<T>);
        let self_ty: syn::Type = syn::parse_quote!(dyn Opaque<T>);
        validate_dispatched_self_ty(&generics, &self_ty).unwrap();
    }

    #[test]
    fn rejects_dispatched_self_type_param() {
        let generics: syn::Generics = syn::parse_quote!(<T>);
        let self_ty: syn::Type = syn::parse_quote!(dyn T);

        let err = validate_dispatched_self_ty(&generics, &self_ty).unwrap_err();
        assert!(err.to_string().contains(DYN_SELF_DECLARED_TYPE_ERR));
    }

    #[test]
    fn rejects_dispatch_param_associated_type() {
        let param: syn::TypeParam = syn::parse_quote!(T);
        let sig: syn::Signature = syn::parse_quote!(fn dispatch(value: T::Assoc));

        let err = validate_dispatch_param_positions([&param].into_iter(), &sig).unwrap_err();
        assert!(err.to_string().contains(DISPATCH_WRAPPER_ERR));
    }

    #[test]
    fn rejects_qualified_dispatch_param_associated_type() {
        let param: syn::TypeParam = syn::parse_quote!(T);
        let sig: syn::Signature = syn::parse_quote!(fn dispatch(value: <T as Trait>::Assoc));

        let err = validate_dispatch_param_positions([&param].into_iter(), &sig).unwrap_err();
        assert!(err.to_string().contains(DISPATCH_WRAPPER_ERR));
    }
}
