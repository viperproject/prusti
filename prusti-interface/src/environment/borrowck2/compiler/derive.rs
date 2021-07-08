//! Derive additional information from the extracted one.

use std::collections::HashMap;
use rustc_middle::{mir, ty, ty::TyCtxt};
use rustc_span::Symbol;
use rustc_mir::borrow_check::location::LocationIndex;

pub(super) fn extract_local_names<'tcx>(body: &mir::Body<'tcx>) -> HashMap<mir::Local, String> {
    let mut names = HashMap::new();
    for info in &body.var_debug_info {
        if let mir::VarDebugInfoContents::Place(place) = info.value {
            if let Some(local) = place.as_local() {
                names.insert(local, info.name.to_string());
            } else {
                unimplemented!();
            }
        } else {
            unimplemented!();
        }
    }
    names
}

pub(super) fn compute_outlives_map(outlives: &[(ty::RegionVid, ty::RegionVid, LocationIndex)]) -> HashMap<LocationIndex, Vec<(ty::RegionVid, ty::RegionVid)>> {
    let mut outlives_map: HashMap<LocationIndex, Vec<(ty::RegionVid, ty::RegionVid)>> = HashMap::new();
    for &(region1, region2, location) in outlives {
        let entry = outlives_map.entry(location).or_default();
        entry.push((region1, region2));
    }
    outlives_map
}