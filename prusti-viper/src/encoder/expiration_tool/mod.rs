// TODO: Remove once top-level lib.rs enables warnings.
#![warn(warnings)]

use std::collections::HashMap;
use std::collections::HashSet;

use itertools::Itertools;
use typed_arena::Arena;

use prusti_interface::specs::typed;

use crate::encoder::places;
use crate::utils::namespace::Namespace;

mod construct;
pub mod encode;
mod pledges;
mod display;
mod split_reborrows;

type Pledge<'tcx> = typed::Assertion<'tcx>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Context {
    BeforeExpiry, AfterUnblocked
}

#[derive(Default)]
pub struct ExpirationToolCarrier<'c, 'tcx> {
    /// A mapping from places to integers that is used to represent places in the Viper encoding.
    place_mapping: HashMap<places::Place<'tcx>, usize>,
    /// Arena for all the expiration tool lists owned by this carrier.
    expiration_tool: Arena<ExpirationTool<'c, 'tcx>>,
    /// Arena for all the expiration tools owned by this carrier.
    partial_expiration_tool: Arena<PartialExpirationTool<'c, 'tcx>>,
    /// Arena for all the magic wands owned by this carrier.
    magic_wand: Arena<MagicWand<'c, 'tcx>>,
    /// Arena for all the pledges owned by this carrier.
    pledge: Arena<Pledge<'tcx>>,
    /// Arena for all namespaces owned by this carrier.
    namespace: Arena<Namespace>
}

/// A collection of expiration tools, one for every connected component of input/output references.
#[derive(Debug, Clone)]
pub struct ExpirationTool<'c, 'tcx> {
    carrier: &'c ExpirationToolCarrier<'c, 'tcx>,
    partial_expiration_tools: Vec<&'c PartialExpirationTool<'c, 'tcx>>
}

/// This is a high-level representation of the nested magic wands that are returned from a
/// re-borrowing function. It has the same structure as the the corresponding Viper expression, but
/// makes the individual components that make up this expression explicit.
#[derive(Debug, Clone)]
pub struct PartialExpirationTool<'c, 'tcx> {
    carrier: &'c ExpirationToolCarrier<'c, 'tcx>,
    /// The places that are still blocking something.
    blocking: HashSet<places::Place<'tcx>>,
    /// The places that are still blocked by something.
    blocked: HashSet<places::Place<'tcx>>,
    /// The magic wands that can be used to expire the places in `blocking` and unblock the places
    /// in `blocked`. For every reference `r` of `blocking`, there is a magic wand `magic_wands[r]`
    /// that is used to expire `r`.
    magic_wands: Vec<&'c MagicWand<'c, 'tcx>>,
}

/// This is a high-level representation of a single magic wand as it appears in the expiration
/// tool. It contains the necessary information to build the left- and right-hand side of the
/// concrete magic wand, but conceptually separated to facilitate manipulation.
#[derive(Debug, Clone)]
pub struct MagicWand<'c, 'tcx> {
    carrier: &'c ExpirationToolCarrier<'c, 'tcx>,
    /// The reference that is expired by applying this magic wand. During encoding, permission for
    /// this will place will make up the left-hand side of the magic wand.
    expired: places::Place<'tcx>,
    /// The references that are immediately unblocked by applying this magic wand. During encoding,
    /// permission for these places will appear on the right-hand side of the magic wand.
    unblocked: HashSet<places::Place<'tcx>>,
    /// The pledges that are made available by applying this magic wand. During encoding, they will
    /// be embedded on the right-hand side of the magic wand. Every pledge has an associated
    /// namespace that it can use for open bindings.
    pledges: Vec<(&'c Pledge<'tcx>, &'c Namespace)>,
    /// The expiration tools that can be used to expire further references. During encoding, they
    /// will be included on the right-hand side of the magic wand.
    expiration_tool: &'c ExpirationTool<'c, 'tcx>,
}

impl<'c, 'tcx> ExpirationToolCarrier<'c, 'tcx> {
    fn add_expiration_tool(&'c self,
        partial_expiration_tools: Vec<&'c PartialExpirationTool<'c, 'tcx>>
    ) -> &'c ExpirationTool<'c, 'tcx> {
        let expiration_tool = ExpirationTool { carrier: self, partial_expiration_tools };
        self.expiration_tool.alloc(expiration_tool)
    }
    fn add_partial_expiration_tool(&'c self,
        blocking: HashSet<places::Place<'tcx>>,
        blocked: HashSet<places::Place<'tcx>>,
        magic_wands: Vec<&'c MagicWand<'c, 'tcx>>
    ) -> &'c PartialExpirationTool<'c, 'tcx> {
        let partial_expiration_tool = PartialExpirationTool {
            carrier: self,
            blocking, blocked, magic_wands
        };
        self.partial_expiration_tool.alloc(partial_expiration_tool)
    }
    fn add_magic_wand(&'c self,
        expired: places::Place<'tcx>,
        unblocked: HashSet<places::Place<'tcx>>,
        pledges: Vec<(&'c Pledge<'tcx>, &'c Namespace)>,
        expiration_tool: &'c ExpirationTool<'c, 'tcx>
    ) -> &'c MagicWand<'c, 'tcx> {
        let magic_wand = MagicWand {
            carrier: self,
            expired, unblocked, pledges, expiration_tool
        };
        self.magic_wand.alloc(magic_wand)
    }
    fn add_pledge(&'c self, pledge: Pledge<'tcx>) -> &'c Pledge<'tcx> {
        self.pledge.alloc(pledge)
    }
    fn add_namespace(&'c self, namespace: Namespace) -> &'c Namespace {
        self.namespace.alloc(namespace)
    }
}

impl<'c, 'p, 'tcx: 'p> ExpirationTool<'c, 'tcx> {
    pub fn blocking(&'c self) -> HashSet<&'c places::Place<'tcx>> {
        self.into_iter().flat_map(|et| &et.blocking).collect()
    }

    pub fn blocked(&'c self) -> HashSet<&'c places::Place<'tcx>> {
        self.into_iter().flat_map(|et| &et.blocked).collect()
    }

    /// Creates an iterator over all partial expiration tools that is ordered deterministically.
    /// This is important during the encoding, where the order of conjuncts in magic wands matters.
    pub fn partial_expiration_tools(&self
    ) -> impl Iterator<Item=&'c PartialExpirationTool<'c, 'tcx>>
    {
        self.into_iter().sorted_by_key(|et| et.blocking.iter().min())
    }

    /// Give us the expiration tool that represents the state after all the given places have
    /// expired.
    pub fn expire(&'c self,
        places: impl IntoIterator<Item=&'p places::Place<'tcx>>
    ) -> &'c ExpirationTool<'c, 'tcx> {
        let mut places = places.into_iter();
        if let Some(place) = places.next() {
            let expiration_tool = self.into_iter().flat_map(|et|
                if let Some(ets) = et.expire(place) {
                    ets.partial_expiration_tools.clone()
                } else {
                    vec![et]
                }
            ).collect();
            self.carrier.add_expiration_tool(expiration_tool).expire(places)
        } else {
            self
        }
    }

    /// Produces the magic wand that expires the given place.
    pub fn magic_wand(&'c self, place: &'p places::Place<'tcx>) -> Option<&'c MagicWand<'c, 'tcx>> {
        self.into_iter().flat_map(|et| et.magic_wand(place)).next()
    }
}

impl<'c, 'tcx> IntoIterator for &ExpirationTool<'c, 'tcx> {
    type Item = &'c PartialExpirationTool<'c, 'tcx>;
    type IntoIter = std::vec::IntoIter<&'c PartialExpirationTool<'c, 'tcx>>;
    fn into_iter(self) -> Self::IntoIter {
        self.partial_expiration_tools.clone().into_iter()
    }
}

impl<'c, 'p, 'tcx: 'p> PartialExpirationTool<'c, 'tcx> {
    /// Creates an iterator over all magic wands that is ordered deterministically. This is
    /// important during the encoding, where the order of conjuncts in magic wands matters.
    fn magic_wands(&self) -> impl Iterator<Item=&'c MagicWand<'c, 'tcx>> {
        self.magic_wands.iter().map(|mw| *mw).sorted_by_key(|mw| mw.expired())
    }

    fn magic_wand(&'c self,
        place: &'p places::Place<'tcx>
    ) -> Option<&'c MagicWand<'c, 'tcx>> {
        self.magic_wands().filter(|mw| mw.expired() == place).next()
    }

    fn expire(&'c self,
        place: &'p places::Place<'tcx>
    ) -> Option<&'c ExpirationTool<'c, 'tcx>> {
        self.magic_wand(place).map(|mw| mw.expiration_tool)
    }
}

impl<'c, 'tcx> MagicWand<'c, 'tcx> {
    /// Returns the reference that is expired by this magic wand. If there is more than one such
    /// reference, it panics.
    pub fn expired(&self) -> &places::Place<'tcx> {
        &self.expired
    }

    /// Creates an iterator over all unblocked references that is ordered deterministically. This
    /// is important during the encoding, where the order of conjuncts in magic wands matters.
    pub fn unblocked(&self) -> impl Iterator<Item=&places::Place<'tcx>> {
        self.unblocked.iter().sorted()
    }

    /// Creates an iterator over all blocking references.
    pub fn blocking(&self) -> impl Iterator<Item=&places::Place<'tcx>> {
        self.expiration_tool.blocking().into_iter()
    }

    /// Creates an iterator over all pledges that is ordered deterministically. This is important
    /// during the encoding, where the order of conjuncts in magic wands matters.
    pub fn pledges(&self) -> impl Iterator<Item=(&'c Pledge<'tcx>, &'c Namespace)> {
        // TODO: This is not determinisitc yet.
        self.pledges.clone().into_iter()
    }
}
