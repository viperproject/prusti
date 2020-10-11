use rustc_index::vec::Idx;
use rustc_middle::mir;
use rustc_middle::ty;

use super::PlaceAddProjection;

pub trait AllPlacesForLocal<'tcx> where Self: Sized {
    /// Returns all places that are below the given local variable. Right now, this only handles
    /// tuples. For a local variable `_2: u32`, `Place::Local(_2).all_places(&mir) == [_2]`. For a
    /// local variable `_2: (u32, u32)`, `Place::Local(_2).all_places(&mir) == [_2, _2.0, _2.1]`.
    fn all_places_with_ty(self,
        tcx: ty::TyCtxt<'tcx>,
        mir: &mir::Body<'tcx>
    ) -> Vec<(mir::Place<'tcx>, ty::Ty<'tcx>)>;

    fn all_places(self,
        tcx: ty::TyCtxt<'tcx>,
        mir: &mir::Body<'tcx>
    ) -> Vec<mir::Place<'tcx>> {
        self.all_places_with_ty(tcx, mir).into_iter()
            .map(|(place, _)| place)
            .collect()
    }
}

impl<'tcx> AllPlacesForLocal<'tcx> for mir::Local {
    fn all_places_with_ty(self,
        tcx: ty::TyCtxt<'tcx>,
        mir: &mir::Body<'tcx>
    ) -> Vec<(mir::Place<'tcx>, ty::Ty<'tcx>)> {
        let ty = mir.local_decls[self].ty;
        enumerate_all_places(self.into(), ty, tcx)
    }
}

/// Returns all places that are below the given base place. Right now, this only handles tuples.
/// For a local variable `_2: u32`, we have
/// ```ignore
/// all_places(Place::Local(_2), u32) == [_2]
///```
/// For a local variable `_2: (u32, u32)`, we have
/// ```ignore
/// all_places(Place::Local(_2), (u32, u32) == [_2, _2.0, _2.1]
/// ```
pub fn enumerate_all_places<'tcx>(
    base: mir::Place<'tcx>,
    ty: ty::Ty<'tcx>,
    tcx: ty::TyCtxt<'tcx>
) -> Vec<(mir::Place<'tcx>, ty::Ty<'tcx>)> {
    let mut places = vec![(base, ty)];
    if let ty::TyKind::Tuple(types) = ty.kind() {
        for (i, ty) in types.iter().enumerate() {
            let field = mir::Field::new(i);
            let ty = ty.expect_ty();
            let place = tcx.mk_place_field(base, field, ty);
            places.push((place, ty));
        }
    }
    places
}
