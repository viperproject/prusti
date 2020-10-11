use rustc_middle::ty;
use rustc_middle::ty::TypeFoldable;
use rustc_middle::ty::TyCtxt;
use rustc_middle::ty::Ty;
use rustc_middle::ty::TypeFolder;

pub trait EraseAllRegions<'tcx> {
    /// Returns an equivalent value with all free regions removed.
    fn erase_all_regions<T>(self, value: &T) -> T
        where T: TypeFoldable<'tcx>;
}

impl<'tcx> EraseAllRegions<'tcx> for TyCtxt<'tcx> {
    fn erase_all_regions<T>(self, value: &T) -> T
        where T: TypeFoldable<'tcx>
    {
        value.fold_with(&mut RegionEraserVisitor { tcx: self })
    }
}

struct RegionEraserVisitor<'tcx> {
    tcx: TyCtxt<'tcx>,
}

impl<'tcx> TypeFolder<'tcx> for RegionEraserVisitor<'tcx> {
    fn tcx<'b>(&'b self) -> TyCtxt<'tcx> {
        self.tcx
    }

    fn fold_region(&mut self, r: ty::Region<'tcx>) -> ty::Region<'tcx> {
        match *r {
            _ => self.tcx.lifetimes.re_erased,
        }
    }
}
