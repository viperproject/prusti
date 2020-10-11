use std::iter::once;

use rustc_middle::mir;
use rustc_middle::ty;

pub trait PlaceAddProjection<'tcx> {
    fn field(self, tcx: ty::TyCtxt<'tcx>, field: mir::Field, ty: ty::Ty<'tcx>) -> Self;
    fn deref(self, tcx: ty::TyCtxt<'tcx>) -> Self;
    fn project(self, tcx: ty::TyCtxt<'tcx>, projection: mir::PlaceElem<'tcx>) -> Self;
    fn truncate(self, tcx: ty::TyCtxt<'tcx>, n: usize) -> Self;
}

impl<'tcx> PlaceAddProjection<'tcx> for mir::Place<'tcx> {
    fn field(self, tcx: ty::TyCtxt<'tcx>, field: mir::Field, ty: ty::Ty<'tcx>) -> Self {
        self.project(tcx, mir::ProjectionElem::Field(field, ty))
    }

    fn deref(self, tcx: ty::TyCtxt<'tcx>) -> Self {
        self.project(tcx, mir::ProjectionElem::Deref)
    }

    fn project(self, tcx: ty::TyCtxt<'tcx>, projection: mir::PlaceElem<'tcx>) -> Self {
        let local = self.local;
        let projection = extend_projection(tcx, self.projection, projection);
        mir::Place { local, projection }
    }

    fn truncate(self, tcx: ty::TyCtxt<'tcx>, n: usize) -> Self {
        assert!(n <= self.projection.len());
        let local = self.local;
        let projection = truncate_projection(tcx, self.projection, n);
        mir::Place { local, projection }
    }
}

fn extend_projection<'tcx>(
    tcx: ty::TyCtxt<'tcx>,
    projection: &'tcx ty::List<mir::PlaceElem<'tcx>>,
    extension: mir::PlaceElem<'tcx>
) -> &'tcx ty::List<mir::PlaceElem<'tcx>> {
    let projection = projection.iter()
        .chain(once(extension))
        .collect::<Vec<_>>();
    tcx.intern_place_elems(&projection)
}

fn truncate_projection<'tcx>(
    tcx: ty::TyCtxt<'tcx>,
    projection: &'tcx ty::List<mir::PlaceElem<'tcx>>,
    n: usize
) -> &'tcx ty::List<mir::PlaceElem<'tcx>> {
    let projection = projection.iter()
        .take(projection.len() - n)
        .collect::<Vec<_>>();
    tcx.intern_place_elems(&projection)
}
