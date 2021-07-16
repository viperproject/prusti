// © 2020, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Various helper functions for working with `mir::Place`.
//! copied from prusti-interface/utils

use std::collections::HashSet;
use rustc_middle::mir;
use rustc_middle::ty::{self, TyCtxt};
use log::trace;


/// Check if the place `potential_prefix` is a prefix of `place`. For example:
///
/// +   `is_prefix(x.f, x.f) == true`
/// +   `is_prefix(x.f.g, x.f) == true`
/// +   `is_prefix(x.f, x.f.g) == false`
pub(crate) fn is_prefix(place: &mir::Place, potential_prefix: &mir::Place) -> bool {
    if place.local != potential_prefix.local || place.projection.len() < potential_prefix.projection.len() {
        false
    } else {
        place.projection.iter().zip(potential_prefix.projection.iter()).all(|(e1, e2)| e1 == e2)
    }
}

/// Expands a place `x.f.g` of type struct into a vector of places for
/// each of the struct's fields `{x.f.g.f, x.f.g.g, x.f.g.h}`. If
/// `without_field` is not `None`, then omits that field from the final
/// vector.
pub(crate) fn expand_struct_place<'tcx>(
    place: &mir::Place<'tcx>,
    mir: &mir::Body<'tcx>,
    tcx: TyCtxt<'tcx>,
    without_field: Option<usize>,
) -> Vec<mir::Place<'tcx>> {
    let mut places = Vec::new();
    let typ = place.ty(mir, tcx);
    if typ.variant_index.is_some() {
        // Downcast is a no-op.
    } else {
        match typ.ty.kind() {
            ty::Adt(def, substs) => {
                assert!(
                    def.is_struct(),
                    "Only structs can be expanded. Got def={:?}.",
                    def
                );
                let variant = def.non_enum_variant();
                for (index, field_def) in variant.fields.iter().enumerate() {
                    if Some(index) != without_field {
                        let field = mir::Field::from_usize(index);
                        let field_place = tcx.mk_place_field(*place, field, field_def.ty(tcx, substs));
                        places.push(field_place);
                    }
                }
            }
            ty::Tuple(slice) => {
                for (index, arg) in slice.iter().enumerate() {
                    if Some(index) != without_field {
                        let field = mir::Field::from_usize(index);
                        let field_place = tcx.mk_place_field(*place, field, arg.expect_ty());
                        places.push(field_place);
                    }
                }
            },
            ty::Ref(_region, _ty, _) => match without_field {
                Some(without_field) => {
                    assert_eq!(
                        without_field, 0,
                        "References have only a single “field”."
                    );
                }
                None => {
                    places.push(tcx.mk_place_deref(*place));
                }
            },
            ref ty => {
                unimplemented!("ty={:?}", ty);
            }
        }
    }
    places
}

/// Expand `current_place` one level down by following the `guide_place`.
/// Returns the new `current_place` and a vector containing other places that
/// could have resulted from the expansion.
pub(crate) fn expand_one_level<'tcx>(
    mir: &mir::Body<'tcx>,
    tcx: TyCtxt<'tcx>,
    current_place: mir::Place<'tcx>,
    guide_place: mir::Place<'tcx>,
) -> (mir::Place<'tcx>, Vec<mir::Place<'tcx>>) {
    let index = current_place.projection.len();
    match guide_place.projection[index] {
        mir::ProjectionElem::Field(projected_field, field_ty) => {
            let places =
                expand_struct_place(&current_place, mir, tcx, Some(projected_field.index()));
            let new_current_place = tcx.mk_place_field(current_place, projected_field, field_ty);
            (new_current_place, places)
        }
        mir::ProjectionElem::Downcast(_symbol, variant) => {
            let kind = &current_place.ty(mir, tcx).ty.kind();
            if let ty::TyKind::Adt(adt, _) = kind {
                (tcx.mk_place_downcast(current_place, adt, variant), Vec::new())
            } else {
                unreachable!();
            }
        }
        mir::ProjectionElem::Deref => {
            (tcx.mk_place_deref(current_place), Vec::new())
        }
        mir::ProjectionElem::Index(idx) => {
            (tcx.mk_place_index(current_place, idx), Vec::new())
        }
        elem => {
            unimplemented!("elem = {:?}", elem);
        }
    }
}

/// Subtract the `subtrahend` place from the `minuend` place. The
/// subtraction is defined as set minus between `minuend` place replaced
/// with a set of places that are unrolled up to the same level as
/// `subtrahend` and the singleton `subtrahend` set. For example,
/// `subtract(x.f, x.f.g.h)` is performed by unrolling `x.f` into
/// `{x.g, x.h, x.f.f, x.f.h, x.f.g.f, x.f.g.g, x.f.g.h}` and
/// subtracting `{x.f.g.h}` from it, which results into `{x.g, x.h,
/// x.f.f, x.f.h, x.f.g.f, x.f.g.g}`.
pub(crate) fn expand<'tcx>(
    mir: &mir::Body<'tcx>,
    tcx: TyCtxt<'tcx>,
    minuend: &mir::Place<'tcx>,
    subtrahend: &mir::Place<'tcx>,
) -> Vec<mir::Place<'tcx>> {
    assert!(
        is_prefix(subtrahend, minuend),
        "The minuend must be the prefix of the subtrahend."
    );
    trace!(
        "[enter] expand minuend={:?} subtrahend={:?}",
        minuend,
        subtrahend
    );
    let mut place_set = Vec::new();
    let mut minuend = *minuend;
    while minuend.projection.len() < subtrahend.projection.len() {
        let (new_minuend, places) = expand_one_level(mir, tcx, minuend, *subtrahend);
        minuend = new_minuend;
        place_set.extend(places);
    }
    trace!(
        "[exit] expand minuend={:?} subtrahend={:?} place_set={:?}",
        minuend,
        subtrahend,
        place_set
    );
    place_set
}

/// Try to collapse all places in `places` by following the
/// `guide_place`. This function is basically the reverse of
/// `expand_struct_place`.
pub(crate) fn collapse<'tcx>(
    mir: &mir::Body<'tcx>,
    tcx: TyCtxt<'tcx>,
    places: &mut HashSet<mir::Place<'tcx>>,
    guide_place: &mir::Place<'tcx>,
) {
    let guide_place = *guide_place;
    fn recurse<'tcx>(
        mir: &mir::Body<'tcx>,
        tcx: TyCtxt<'tcx>,
        places: &mut HashSet<mir::Place<'tcx>>,
        current_place: mir::Place<'tcx>,
        guide_place: mir::Place<'tcx>,
    ) {
        if current_place == guide_place {
            return;
        } else {
            let (new_current_place, mut expansion) = expand_one_level(
                mir, tcx, current_place, guide_place);
            recurse(mir, tcx, places, new_current_place, guide_place);
            expansion.push(new_current_place);
            if expansion.iter().all(|place| places.contains(place)) {
                for place in expansion {
                    places.remove(&place);
                }
                places.insert(current_place);
            } else {
                return;
            }
        }
    }
    recurse(mir, tcx, places, guide_place.local.into(), guide_place);
}
