// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

///! Code for finding `rustc::ty::sty::RegionVid` associated with local
///! reference typed variables.

use rustc_mir::consumers::BodyWithBorrowckFacts;

use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::io::BufRead;
use std::path::Path;

use log::debug;
use log::trace;
use regex::Regex;

use rustc_index::vec::Idx;
use rustc_middle::{mir, ty};

use crate::environment::borrowck::facts;

#[derive(Debug)]
pub struct PlaceRegions(HashMap<(mir::Local, Vec<usize>), facts::Region>);

#[derive(Clone, Debug)]
pub enum PlaceRegionsError {
    Unsupported(String),
}

impl PlaceRegions {
    fn new() -> Self {
        PlaceRegions(HashMap::new())
    }

    fn add_local(&mut self, local: mir::Local, rvid: facts::Region) {
        self.add(local, vec![], rvid);
    }

    fn add(&mut self, local: mir::Local, projections: Vec<usize>, rvid: facts::Region) {
        self.0.insert((local, projections), rvid);
    }

    pub fn for_local(&self, local: mir::Local)-> Option<facts::Region> {
        self.for_place(local.into()).unwrap()
    }

    /// Determines the region of a MIR place. Right now, the only supported places are locals and tuples. Tuples cannot be nested inside other tuples.
    pub fn for_place(&self, place: mir::Place)
        -> Result<Option<facts::Region>, PlaceRegionsError>
    {
        let (local, fields) = Self::translate_place(place)?;
        Ok(self.0.get(&(local, fields)).cloned())
    }

    /// Translates a place like _3.0.3.1 into a local (here _3) and a list of field projections like (here [0, 3, 1]).
    fn translate_place(place: mir::Place)
        -> Result<(mir::Local, Vec<usize>), PlaceRegionsError>
    {
        let indices = place.projection.iter()
            .map(|elem| match elem {
                mir::ProjectionElem::Field(f, _) => Ok(f.index()),
                mir::ProjectionElem::Deref => {
                    Err(PlaceRegionsError::Unsupported(
                        "determining the region of a dereferentiation is \
                        not supported".to_string()
                    ))
                }
                mir::ProjectionElem::Index(_) => {
                    Err(PlaceRegionsError::Unsupported(
                        "determining the region of array indexing is \
                        not supported".to_string()
                    ))
                }
                mir::ProjectionElem::ConstantIndex{..} => {
                    Err(PlaceRegionsError::Unsupported(
                        "determining the region of constant indexing is \
                        not supported".to_string()
                    ))
                }
                mir::ProjectionElem::Subslice{..} => {
                    Err(PlaceRegionsError::Unsupported(
                        "determining the region of a subslice is \
                        not supported".to_string()
                    ))
                }
                mir::ProjectionElem::Downcast(_, _) => {
                    Err(PlaceRegionsError::Unsupported(
                        "determining the region of a downcast is \
                        not supported".to_string()
                    ))
                }
            })
            .collect::<Result<_, _>>()?;
        Ok((place.local, indices))
    }
}

fn extract_region_id(region: &ty::RegionKind) -> facts::Region {
    match region {
        ty::RegionKind::ReVar(rvid) => {
            *rvid
        },
        _ => unimplemented!("region: {:?}", region),
    }
}

fn extract_region(place_regions: &mut PlaceRegions, local: mir::Local, ty: ty::Ty<'_>) {
    match ty.kind() {
        ty::TyKind::Ref(region, _, _) => {
            place_regions.add_local(local, extract_region_id(region));
            debug!("region: {:?}", region);
        }
        ty::TyKind::Tuple(substs) => {
            for (i, ty) in substs.types().enumerate() {
                match ty.kind() {
                    ty::TyKind::Ref(region, _, _) => {
                        place_regions.add(local, vec![i], extract_region_id(region))
                    }
                    _ => {
                        debug!("does not contain regions: {:?}[{}]: {:?} {:?}", local, i, ty, ty.kind());
                    }
                }

            }
        }
        _ => {
            debug!("does not contain regions: {:?}: {:?} {:?}", local, ty, ty.kind());
        }
    }
}

pub fn load_place_regions(body: &mir::Body<'_>, facts: &facts::BorrowckFacts) -> io::Result<PlaceRegions> {
    trace!("[enter] load_place_regions()");
    let mut place_regions = PlaceRegions::new();

    for (local, local_decl) in body.local_decls.iter_enumerated() {
        let ty = local_decl.ty;
        debug!("local: {:?} {:?}", local, ty);
        extract_region(&mut place_regions,  local, ty);
    }

    trace!("[exit] load_place_regions");
    Ok(place_regions)
}
