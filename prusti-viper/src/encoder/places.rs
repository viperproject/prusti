// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use rustc_middle::mir;
use rustc_middle::ty::Ty;
use rustc_index::vec::{Idx, IndexVec, IntoIdx};
use std::{iter, ops};

/// A local variable used as an abstraction over both real Rust MIR local
/// variables and temporary variables used in encoder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Local(u32);

impl Idx for Local {
    fn new(idx: usize) -> Self {
        Local(idx as u32)
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

impl From<mir::Local> for Local {
    fn from(other: mir::Local) -> Self {
        Local::new(other.index())
    }
}

impl Into<mir::Local> for Local {
    fn into(self) -> mir::Local {
        mir::Local::new(self.index())
    }
}

/// Information about a local variable.
#[derive(Debug)]
pub enum LocalVarData<'tcx> {
    RealLocal(mir::Local, mir::LocalDecl<'tcx>),
    TempLocal { ty: Ty<'tcx> },
}

/// Struct that keeps track of all local variables.
pub struct LocalVariableManager<'tcx> {
    variables: IndexVec<Local, LocalVarData<'tcx>>,
}

impl<'tcx> LocalVariableManager<'tcx> {
    pub fn new(real_locals: &IndexVec<mir::Local, mir::LocalDecl<'tcx>>) -> Self {
        let mut manager = LocalVariableManager {
            variables: IndexVec::new(),
        };
        for (real_local, real_local_decl) in real_locals.iter_enumerated() {
            let index = manager
                .variables
                .push(LocalVarData::RealLocal(real_local, real_local_decl.clone()));
            assert_eq!(real_local.index(), index.index());
        }
        manager
    }

    /// Create a fresh temporary variable of a given type.
    pub fn get_fresh(&mut self, ty: Ty<'tcx>) -> Local {
        self.variables.push(LocalVarData::TempLocal { ty })
    }

    // TODO: Can we try to go through MirEncoder::encode_local_var_name?
    pub fn get_name(&self, local: Local) -> String {
        match self.variables[local] {
            LocalVarData::RealLocal(_, _) => format!("_{}", local.0),
            LocalVarData::TempLocal { .. } => format!("_t{}", local.0),
        }
    }

    pub fn get_type(&self, local: Local) -> Ty<'tcx> {
        match self.variables[local] {
            LocalVarData::RealLocal(_, ref decl) => decl.ty,
            LocalVarData::TempLocal { ty } => ty,
        }
    }

    /// True if the local variable is the formal return of the procedure.
    pub fn is_return(&self, local: Local) -> bool {
        match self.variables[local] {
            LocalVarData::RealLocal(real_local, _) => real_local == mir::RETURN_PLACE,
            _ => false,
        }
    }

    pub fn iter(&self) -> iter::Map<ops::Range<usize>, IntoIdx<Local>> {
        self.variables.indices()
    }
}

/// This place is a generalisation of mir::Place.
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Clone)]
pub enum Place<'tcx> {
    /// A place that is a MIR place.
    NormalPlace(mir::Place<'tcx>),
    /// A place that is a MIR place starting at a specific local variable
    /// instead of its normal root.
    ///
    /// For example, if `substituted_root = _g5` and
    /// `place = _3.f`, then in Viper this place would be encoded as
    /// `_g5.val_ref.T$f.val_ref`
    SubstitutedPlace {
        substituted_root: Local,
        place: mir::Place<'tcx>,
    },
}

impl<'a, 'tcx: 'a> From<&'a mir::Place<'tcx>> for Place<'tcx> {
    fn from(other: &'a mir::Place<'tcx>) -> Self {
        other.clone().into()
    }
}

impl<'tcx> From<mir::Place<'tcx>> for Place<'tcx> {
    fn from(other: mir::Place<'tcx>) -> Self {
        Place::NormalPlace(other)
    }
}

impl<'tcx> Place<'tcx> {
    pub fn new_normal(place: mir::Place<'tcx>) -> Self {
        Self::NormalPlace(place)
    }

    pub fn new_substituted(substituted_root: Local, place: mir::Place<'tcx>) -> Self {
        Self::SubstitutedPlace { substituted_root, place }
    }

    pub fn from_place(original_root: mir::Local, mut place: mir::Place<'tcx>) -> Self {
        let substituted_root = Local::new(place.local.index());
        place.local = original_root;
        Self::new_substituted(substituted_root, place)
    }

    pub fn is_root(&self, local: Local) -> bool {
        // fn check_if_root(place: &mir::Place, local: Local) -> bool {
        //     match place {
        //         mir::Place::Local(root) => local.index() == root.index(),
        //         mir::Place::Projection(box mir::Projection { ref base, .. }) => {
        //             check_if_root(base, local)
        //         }
        //         _ => unimplemented!(),
        //     }
        // }
        match self {
            // Place::NormalPlace(ref place) => check_if_root(place, local),
            Place::NormalPlace(ref place) => place.local.index() == local.index(),
            Place::SubstitutedPlace {
                substituted_root, ..
            } => *substituted_root == local,
        }
    }

    pub fn to_mir_place(&self) -> mir::Place<'tcx> {
        match self.clone() {
            Place::NormalPlace(place) => place,
            Place::SubstitutedPlace { substituted_root, mut place } => {
                place.local = substituted_root.into();
                place
            }
        }
    }

    pub fn as_normal_place(&self) -> Option<&mir::Place<'tcx>> {
        if let Place::NormalPlace(place) = self { Some(place) } else { None }
    }
}
