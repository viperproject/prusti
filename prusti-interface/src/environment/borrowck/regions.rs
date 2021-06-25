// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

///! Code for finding `rustc::ty::sty::RegionVid` associated with local
///! reference typed variables.

pub use rustc_mir::consumers::{compute_polonius_facts, BodyWithFacts};

use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::io::BufRead;
use std::path::Path;

use log::debug;
use log::trace;
use regex::Regex;

use rustc_index::vec::Idx;
use rustc_middle::mir;

use crate::environment::borrowck::facts;

// lazy_static! {
//     static ref FN_SIG: Regex =
//         Regex::new(r"^fn [a-zA-Z\d_]+\((?P<args>.*)\) -> (?P<result>.*)\{$").unwrap();

//     static ref ARG: Regex =
//         Regex::new(r"^_(?P<local>\d+): &'_#(?P<rvid>\d+)r (mut)? [a-zA-Z\d_]+\s*$").unwrap();

//     static ref LOCAL: Regex =
//         Regex::new(r"let( mut)? _(?P<local>\d+): &'_#(?P<rvid>\d+)r ").unwrap();

//     static ref LOCAL_TUPLE: Regex =
//         Regex::new(r"let( mut)? _(?P<local>\d+): \((?P<items>.*)\);").unwrap();

//     static ref REF: Regex = Regex::new(r"&'_#(?P<rvid>\d+)r ").unwrap();
// }

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

pub fn load_place_regions(path: &BodyWithFacts) -> io::Result<PlaceRegions> {
    trace!("[enter] load_place_regions()");
    let mut place_regions = PlaceRegions::new();

    unimplemented!("TODO");
    // let file = File::open(path)?;
    // for line in io::BufReader::new(file).lines() {
    //     let line = line?;
    //     regions_for_fn_sig(&mut place_regions, &line);
    //     regions_for_local_ref(&mut place_regions, &line);
    //     regions_for_local_tuple(&mut place_regions, &line)
    // }
    trace!("[exit] load_place_regions");
    Ok(place_regions)
}

// /// This loads regions for parameters and return values in function signatures.
// fn regions_for_fn_sig(place_regions: &mut PlaceRegions, line: &String) {
//     if let Some(caps) = FN_SIG.captures(&line) {
//         debug!("args: {} result: {}", &caps["args"], &caps["result"]);
//         for arg_str in (&caps["args"]).split(", ") {
//             if let Some(arg_caps) = ARG.captures(arg_str) {
//                 debug!("arg {} rvid {}", &arg_caps["local"], &arg_caps["rvid"]);
//                 let local_arg: usize = (&arg_caps["local"]).parse().unwrap();
//                 let rvid: usize = (&arg_caps["rvid"]).parse().unwrap();
//                 place_regions.add_local(mir::Local::new(local_arg), rvid.into());
//             }
//         }
//     }
// }

// /// This loads regions for reference-typed local variables. For a local variable declaration like
// ///   let _3: &'2rv mut i32;
// /// it would record that the place _3 has region 2.
// fn regions_for_local_ref(place_regions: &mut PlaceRegions, line: &String) {
//     if let Some(local_caps) = LOCAL.captures(&line) {
//         debug!(
//             "local {} rvid {}",
//             &local_caps["local"], &local_caps["rvid"]
//         );
//         let local_arg: usize = (&local_caps["local"]).parse().unwrap();
//         let rvid: usize = (&local_caps["rvid"]).parse().unwrap();
//         place_regions.add_local(mir::Local::new(local_arg), rvid.into());
//     }
// }

// /// This loads regions for tuples. For a local variable declaration like
// /// ```ignore
// /// let _5: (&'6rv mut i32, &'7rv mut i32);
// /// ```
// /// it would record that the place _5.0 has region 6 and the place _5.1 has region 7.
// fn regions_for_local_tuple(place_regions: &mut PlaceRegions, line: &String) {
//     if let Some(m) = LOCAL_TUPLE.captures(&line) {
//         let local = mir::Local::new(m["local"].parse().unwrap());
//         let items = &m["items"];
//         for (i, m) in REF.captures_iter(&items).enumerate() {
//             let rvid: usize = m["rvid"].parse().unwrap();
//             place_regions.add(local, vec![i], rvid.into());
//         }
//     }
// }
