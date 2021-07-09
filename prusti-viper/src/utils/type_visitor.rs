// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use rustc_hir::Mutability;
use rustc_middle::ty::{
    AdtDef, FieldDef, ParamTy, ProjectionTy, Region, Slice, Ty, TyCtxt,
    TypeFlags, TyKind, IntTy, UintTy, FloatTy, VariantDef, subst::SubstsRef, Const
};
use rustc_hir::def_id::DefId;
use log::trace;

pub trait TypeVisitor<'tcx>: Sized {
    type Error;

    fn tcx(&self) -> TyCtxt<'tcx>;

    fn unsupported<S: ToString>(&self, _msg: S) -> Self::Error;

    fn visit_ty(&mut self, ty: Ty<'tcx>) -> Result<(), Self::Error> {
        trace!("visit_ty({:?})", ty);
        self.visit_sty(ty.kind())?;
        self.visit_flags(ty.flags())
    }

    fn visit_sty(&mut self, sty: &TyKind<'tcx>) -> Result<(), Self::Error> {
        trace!("visit_sty({:?})", sty);
        match *sty {
            TyKind::Bool => {
                self.visit_bool()
            }
            TyKind::Int(ty) => {
                self.visit_int(ty)
            }
            TyKind::Uint(ty) => {
                self.visit_uint(ty)
            }
            TyKind::Char => {
                self.visit_char()
            }
            TyKind::Adt(adt_def, substs) => {
                self.visit_adt(adt_def, substs)
            }
            TyKind::Ref(region, ty, mutability) => {
                self.visit_ref(region, ty, mutability)
            }
            TyKind::Tuple(parts) => {
                self.visit_tuple(parts)
            }
            TyKind::RawPtr(ty_and_mutbl) => {
                self.visit_raw_ptr(ty_and_mutbl.ty, ty_and_mutbl.mutbl)
            }
            TyKind::Never => {
                self.visit_never()
            }
            TyKind::Param(param) => {
                self.visit_param(param)
            }
            TyKind::Projection(data) => {
                self.visit_projection(data)
            }
            TyKind::Closure(def_id, substs) => {
                self.visit_closure(def_id, substs)
            }
            TyKind::FnDef(def_id, substs) => {
                self.visit_fndef(def_id, substs)
            }
            TyKind::Array(ty, len) => {
                self.visit_array(ty, len)
            }
            TyKind::Float(fty) => self.visit_float(fty),
            ref x => {
                self.visit_unsupported_sty(x)
            }

        }
    }

    fn visit_unsupported_sty(
        &mut self,
        _sty: &TyKind<'tcx>
    ) -> Result<(), Self::Error>;

    fn visit_flags(&mut self, _flags: TypeFlags) -> Result<(), Self::Error> {
        Ok(())
    }

    fn visit_bool(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn visit_int(&mut self, _ty: IntTy) -> Result<(), Self::Error> {
        Ok(())
    }

    fn visit_uint(&mut self, _ty: UintTy) -> Result<(), Self::Error> {
        Ok(())
    }

    fn visit_char(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn visit_float(&mut self, _ty: FloatTy) -> Result<(), Self::Error> {
        Ok(())
    }

    fn visit_never(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn visit_param(&mut self, _param: ParamTy) -> Result<(), Self::Error> {
        Ok(())
    }

    fn visit_projection(
        &mut self,
        _data: ProjectionTy<'tcx>
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn visit_adt(
        &mut self,
        adt_def: &'tcx AdtDef,
        substs: SubstsRef<'tcx>
    ) -> Result<(), Self::Error> {
        trace!("visit_adt({:?})", adt_def);
        walk_adt(self, adt_def, substs)
    }

    fn visit_adt_variant(
        &mut self,
        variant: &VariantDef,
        substs: SubstsRef<'tcx>,
    )  -> Result<(), Self::Error> {
        trace!("visit_adt_variant({:?})", variant);
        walk_adt_variant(self, variant, substs)
    }

    fn visit_field(
        &mut self,
        index: usize,
        field: &FieldDef,
        substs: SubstsRef<'tcx>,
    ) -> Result<(), Self::Error> {
        trace!("visit_field({}, {:?})", index, field);
        walk_field(self, field, substs)
    }

    fn visit_ref(
        &mut self,
        region: Region<'tcx>,
        ty: Ty<'tcx>,
        mutability: Mutability
    ) -> Result<(), Self::Error> {
        trace!("visit_ref({:?}, {:?}, {:?})", region, ty, mutability);
        walk_ref(self, region, ty, mutability)
    }

    #[allow(dead_code)]
    fn visit_ref_type(
        &mut self,
        ty: Ty<'tcx>,
        mutability: Mutability
    ) -> Result<(), Self::Error> {
        trace!("visit_ref_type({:?}, {:?})", ty, mutability);
        walk_ref_type(self, ty, mutability)
    }

    fn visit_tuple(
        &mut self,
        parts: SubstsRef<'tcx>
    ) -> Result<(), Self::Error> {
        trace!("visit_tuple({:?})", parts);
        walk_tuple(self, parts)
    }

    fn visit_raw_ptr(
        &mut self,
        ty: Ty<'tcx>,
        mutability: Mutability
    ) -> Result<(), Self::Error> {
        trace!("visit_raw_ptr({:?}, {:?})", ty, mutability);
        walk_raw_ptr(self, ty, mutability)
    }

    fn visit_closure(
        &mut self,
        def_id: DefId,
        substs: SubstsRef<'tcx>
    ) -> Result<(), Self::Error> {
        trace!("visit_closure({:?})", def_id);
        walk_closure(self, def_id, substs)
    }

    fn visit_fndef(
        &mut self,
        def_id: DefId,
        substs: SubstsRef<'tcx>
    ) -> Result<(), Self::Error> {
        trace!("visit_fndef({:?})", def_id);
        walk_fndef(self, def_id, substs)
    }

    fn visit_array(
        &mut self,
        ty: Ty<'tcx>,
        len: &'tcx Const<'tcx>,
    ) -> Result<(), Self::Error> {
        trace!("visit_array({:?}, {:?})", ty, len);
        walk_array(self, ty, len)
    }
}

pub fn walk_adt<'tcx, E, V: TypeVisitor<'tcx, Error = E>>(
    visitor: &mut V,
    adt_def: &'tcx AdtDef,
    substs: SubstsRef<'tcx>,
) -> Result<(), E> {
    for variant in adt_def.variants.iter() {
        visitor.visit_adt_variant(variant, substs)?;
    }
    Ok(())
}

pub fn walk_adt_variant<'a, 'tcx, E, V: TypeVisitor<'tcx, Error = E>>(
    visitor: &mut V,
    variant: &VariantDef,
    substs: SubstsRef<'tcx>,
) -> Result<(), E> {
    for (index, field) in variant.fields.iter().enumerate() {
        visitor.visit_field(index, field, substs)?;
    }
    Ok(())
}

pub fn walk_field<'a, 'tcx, E, V: TypeVisitor<'tcx, Error = E>>(
    visitor: &mut V,
    field: &FieldDef,
    substs: SubstsRef<'tcx>,
) -> Result<(), E> {
    let ty = field.ty(visitor.tcx(), substs);
    visitor.visit_ty(ty)
}

pub fn walk_ref<'tcx, E, V: TypeVisitor<'tcx, Error = E>>(
    visitor: &mut V,
    _: Region<'tcx>,
    ty: Ty<'tcx>,
    _: Mutability,
) -> Result<(), E> {
    visitor.visit_ty(ty)
}

pub fn walk_ref_type<'tcx, E, V: TypeVisitor<'tcx, Error = E>>(
    visitor: &mut V,
    ty: Ty<'tcx>,
    _: Mutability,
) -> Result<(), E> {
    visitor.visit_ty(ty)
}

pub fn walk_tuple<'tcx, E, V: TypeVisitor<'tcx, Error = E>>(
    visitor: &mut V,
    parts: SubstsRef<'tcx>,
) -> Result<(), E> {
    for part in parts.iter() {
        visitor.visit_ty(part.expect_ty())?;
    }
    Ok(())
}

pub fn walk_raw_ptr<'tcx, E, V: TypeVisitor<'tcx, Error = E>>(
    visitor: &mut V,
    ty: Ty<'tcx>,
    _: Mutability,
) -> Result<(), E> {
    visitor.visit_ty(ty)
}

pub fn walk_closure<'tcx, E, V: TypeVisitor<'tcx, Error = E>>(
    visitor: &mut V,
    _def_id: DefId,
    substs: SubstsRef<'tcx>
) -> Result<(), E> {
    let cl_substs = substs.as_closure();
    // TODO: when are there bound typevars? can type visitor deal with generics?
    let fn_sig =
        cl_substs.sig()
                 .no_bound_vars()
                 .ok_or_else(|| visitor.unsupported("higher-ranked lifetimes and types are not supported"))?; 
    for ty in fn_sig.inputs() {
        visitor.visit_ty(ty)?;
    }
    visitor.visit_ty(fn_sig.output())
}

pub fn walk_fndef<'tcx, E, V: TypeVisitor<'tcx, Error = E>>(
    visitor: &mut V,
    _def_id: DefId,
    substs: SubstsRef<'tcx>
) -> Result<(), E> {
    for ty in substs.types() {
        visitor.visit_ty(ty)?;
    }
    Ok(())
}

pub fn walk_array<'tcx, E, V: TypeVisitor<'tcx, Error = E>>(
    visitor: &mut V,
    ty: Ty<'tcx>,
    _len: &'tcx Const<'tcx>,
) -> Result<(), E> {
    visitor.visit_ty(ty)?;

    Ok(())
}
