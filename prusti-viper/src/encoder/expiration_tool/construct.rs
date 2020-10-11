use itertools::Either;
use itertools::Itertools;

use prusti_interface::specs::typed;
use rustc_middle::mir;
use rustc_middle::ty;

use crate::encoder::interface_reborrowing_graph::InterfaceReborrowingGraph;
use crate::encoder::places;
use crate::encoder::procedure_encoder::Result;
use crate::utils::namespace::Namespace;

use super::ExpirationTool;
use super::ExpirationToolCarrier;
use super::MagicWand;
use super::PartialExpirationTool;
use super::Pledge;
use super::pledges::identify_dependencies;
use super::pledges::PledgeDependencies;
use super::pledges::PledgeDependenciesSatisfied;
use super::split_reborrows::split_reborrows;

impl<'c, 'tcx> ExpirationToolCarrier<'c, 'tcx> {
    pub fn construct(&'c mut self,
        tcx: ty::TyCtxt<'tcx>,
        mir: &mir::Body<'tcx>,
        reborrows: &InterfaceReborrowingGraph<places::Place<'tcx>>,
        pledges: Vec<typed::Assertion<'tcx>>
    ) -> Result<&'c ExpirationTool<'c, 'tcx>> {
        self.place_mapping = reborrows.blocking.iter().cloned()
            .enumerate().map(|(i, p)| (p, i))
            .collect();
        let shared_self = &*self;
        let mut namespace = Namespace::new("et");
        let pledges = pledges.into_iter()
            .map(|pledge| {
                let pledge = shared_self.add_pledge(pledge);
                let dependencies = identify_dependencies(tcx, mir, &reborrows, pledge)?;
                let namespace = shared_self.add_namespace(namespace.next_child());
                Ok((pledge, (dependencies, namespace)))
            })
            .collect::<Result<Vec<_>>>()?;
        let pledges = pledges.iter()
            .map(|(pledge, attributes)| (*pledge, attributes))
            .collect::<Vec<_>>();
        Ok(ExpirationTool::construct(self, reborrows, &pledges))
    }
}

impl<'c, 'tcx> ExpirationTool<'c, 'tcx> {
    fn construct(
        carrier: &'c ExpirationToolCarrier<'c, 'tcx>,
        reborrows: &InterfaceReborrowingGraph<places::Place<'tcx>>,
        pledges: &[(&'c Pledge<'tcx>, &(PledgeDependencies<'tcx>, &'c Namespace))],
    ) -> &'c Self {
        let connected_components = split_reborrows(
            reborrows, pledges, |(_, (dependencies, _))| dependencies);
        let partial_expiration_tools = connected_components.into_iter()
            .sorted_by_key(|(reborrows, _)| reborrows.blocking.iter().min().cloned())
            .map(|(reborrows, pledges)|
                PartialExpirationTool::construct(carrier, &reborrows, &pledges))
            .collect::<Vec<_>>().into();
        carrier.add_expiration_tool(partial_expiration_tools)
    }
}

impl<'c, 'tcx> PartialExpirationTool<'c, 'tcx> {
    fn construct(
        carrier: &'c ExpirationToolCarrier<'c, 'tcx>,
        reborrows: &InterfaceReborrowingGraph<places::Place<'tcx>>,
        pledges: &[(&'c Pledge<'tcx>, &(PledgeDependencies<'tcx>, &'c Namespace))],
    ) -> &'c Self {
        let blocking = reborrows.blocking.clone();
        let blocked = reborrows.blocked.clone();
        let magic_wands = reborrows.blocking().cloned().map(|expired|
            MagicWand::construct(carrier, reborrows, pledges, expired)).collect();
        carrier.add_partial_expiration_tool(blocking, blocked, magic_wands)
    }
}

impl<'c, 'tcx> MagicWand<'c, 'tcx> {
    fn construct(
        carrier: &'c ExpirationToolCarrier<'c, 'tcx>,
        reborrows: &InterfaceReborrowingGraph<places::Place<'tcx>>,
        pledges: &[(&'c Pledge<'tcx>, &(PledgeDependencies<'tcx>, &'c Namespace))],
        expired: places::Place<'tcx>,
    ) -> &'c Self {
        // Expire the blocking reference and obtain the updated reborrow information.
        let (reborrows, unblocked) = reborrows.expire_output(&expired);

        // The pledges that are made available by this magic wand.
        let (ripe_pledges, pending_pledges): (Vec<_>, Vec<_>) = pledges.into_iter().cloned()
            .partition_map(|item| {
                let (pledge, (dependencies, namespace)) = item;
                let is_satisfied = dependencies.are_satisfied(
                    &reborrows.blocking, &reborrows.blocked);
                if is_satisfied {
                    Either::Left((pledge, *namespace))
                } else {
                    Either::Right(item)
                }
            });

        // The nested expiration tools.
        let expiration_tool = ExpirationTool::construct(carrier, &reborrows, &pending_pledges);

        carrier.add_magic_wand(
            expired, unblocked,
            ripe_pledges,
            expiration_tool
        )
    }
}
