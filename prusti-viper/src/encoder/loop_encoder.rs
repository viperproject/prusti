// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use prusti_interface::environment::mir_analyses::initialization::{
    compute_definitely_initialized, DefinitelyInitializedAnalysisResult,
};
use prusti_interface::environment::place_set::PlaceSet;
use prusti_interface::environment::{BasicBlockIndex, PermissionForest, ProcedureLoops, Procedure};
use prusti_interface::utils;
use rustc_middle::{mir, ty};
use log::{trace, debug};

pub enum LoopEncoderError {
    LoopInvariantInBranch(BasicBlockIndex),
}

pub struct LoopEncoder<'p, 'tcx: 'p> {
    procedure: &'p Procedure<'tcx>,
    tcx: ty::TyCtxt<'tcx>,
    initialization: DefinitelyInitializedAnalysisResult<'tcx>,
}

impl<'p, 'tcx: 'p> LoopEncoder<'p, 'tcx> {
    pub fn new(
        procedure: &'p Procedure<'tcx>,
        tcx: ty::TyCtxt<'tcx>,
    ) -> Self {
        LoopEncoder {
            procedure,
            tcx,
            initialization: compute_definitely_initialized(
                procedure.get_mir(),
                tcx,
            ),
        }
    }

    pub fn mir(&self) -> &mir::Body<'tcx> {
        self.procedure.get_mir()
    }

    pub fn loops(&self) -> &ProcedureLoops {
        &self.procedure.loop_info()
    }

    /// Is the given basic block a loop head?
    pub fn is_loop_head(&self, bbi: BasicBlockIndex) -> bool {
        self.loops().is_loop_head(bbi)
    }

    /// Note: a loop head is loop head of itself
    pub fn get_loop_head(&self, bbi: BasicBlockIndex) -> Option<BasicBlockIndex> {
        self.loops().get_loop_head(bbi)
    }

    /// 0 = outside loops, 1 = inside one loop, 2 = inside 2 loops and so on
    pub fn get_loop_depth(&self, bbi: BasicBlockIndex) -> usize {
        self.get_loop_head(bbi)
            .map(|head| self.loops().get_loop_head_depth(head))
            .unwrap_or(0)
    }

    /// Get iterator over enclosing loop heads.
    pub fn get_enclosing_loop_heads(&self, bbi: BasicBlockIndex) -> &[BasicBlockIndex] {
        self.loops().get_enclosing_loop_heads(bbi)
    }

    pub fn compute_loop_invariant(
        &self,
        bb: BasicBlockIndex,
        bb_inv: BasicBlockIndex
    ) -> PermissionForest<'p, 'tcx> {
        assert!(self.is_loop_head(bb));

        // 1.  Let ``A1`` be a set of pairs ``(p, t)`` where ``p`` is a prefix
        //     accessed in the loop body and ``t`` is the type of access (read,
        //     destructive read, …).
        // 2.  Let ``A2`` be a subset of ``A1`` that contains only the prefixes
        //     whose roots are defined before the loop. (The root of the prefix
        //     ``x.f.g.h`` is ``x``.)
        // 3.  Let ``A3`` be a subset of ``A2`` without accesses that are subsumed
        //     by other accesses.
        // 4.  Let ``U`` be a set of prefixes that are unreachable at the loop
        //     head because they are either moved out or mutably borrowed.
        // 5.  For each access ``(p, t)`` in the set ``A3``:
        //
        //     1.  Add a read permission to the loop invariant to read the prefix
        //         up to the last element. If needed, unfold the corresponding
        //         predicates.
        //     2.  Add a permission to the last element based on what is required
        //         by the type of access. If ``p`` is a prefix of some prefixes in
        //         ``U``, then the invariant would contain corresponding predicate
        //         bodies without unreachable elements instead of predicates.

        // Paths accessed inside the loop body.
        let (write_leaves, mut_borrow_leaves, read_leaves) =
            self.loops().compute_read_and_write_leaves(
                bb,
                self.mir(),
                Some(self.initialization.get_before_block(bb_inv)),
            );

        let mut all_places = PlaceSet::new();
        for place in &read_leaves {
            all_places.insert(&place, self.mir(), self.tcx)
        }
        for place in &mut_borrow_leaves {
            all_places.insert(&place, self.mir(), self.tcx)
        }
        for place in &write_leaves {
            all_places.insert(&place, self.mir(), self.tcx)
        }

        // Construct the permission forest.
        let forest =
            PermissionForest::new(self.procedure.get_mir(), self.tcx, &write_leaves, &mut_borrow_leaves, &read_leaves, &all_places);

        forest
    }

    /// Is the ``place`` definitely initialised at the beginning of ``bbi``?
    pub fn is_definitely_initialised(&self, place: &mir::Place, bbi: BasicBlockIndex) -> bool {
        self.initialization
            .get_before_block(bbi)
            .iter()
            .any(|def_init_place| utils::is_prefix(place, def_init_place))
    }

    /// Return the block at whose end the loop invariant holds
    pub fn get_loop_invariant_block(
        &self,
        loop_head: BasicBlockIndex
    ) -> Result<BasicBlockIndex, LoopEncoderError> {
        trace!("get_loop_special_blocks: {:?}", loop_head);
        let loop_info = self.loops();
        debug_assert!(loop_info.is_loop_head(loop_head));
        let loop_depth = loop_info.get_loop_head_depth(loop_head);

        let loop_body: Vec<BasicBlockIndex> = loop_info
            .get_loop_body(loop_head)
            .iter()
            .filter(|&&bb| !self.procedure.is_spec_block(bb))
            .cloned()
            .collect();

        let loop_exit_blocks = loop_info.get_loop_exit_blocks(loop_head);
        let before_invariant_block: BasicBlockIndex = loop_body
            .iter()
            .find(|&&bb| {
                loop_info.get_loop_depth(bb) == loop_depth
                    && self.mir()[bb].terminator().successors().any(|&succ_bb| {
                        self.procedure.is_reachable_block(succ_bb)
                            && self.procedure.is_spec_block(succ_bb)
                    })
            })
            .cloned()
            .unwrap_or_else(|| loop_exit_blocks.get(0).cloned().unwrap_or(loop_head));

        if loop_info.is_conditional_branch(loop_head, before_invariant_block) {
            debug!(
                "{:?} is conditional branch in loop {:?}",
                before_invariant_block, loop_head
            );
            return Err(LoopEncoderError::LoopInvariantInBranch(loop_head));
        }

        Ok(before_invariant_block)
    }
}
