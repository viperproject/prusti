#![warn(warnings)]

use std::collections::HashMap;
use std::collections::HashSet;
use std::hash::Hash;

use itertools::Itertools;

use prusti_interface::environment::mir_utils::enumerate_all_places;
use prusti_interface::environment::mir_utils::EraseAllRegions;
use prusti_interface::environment::mir_utils::PlaceAddProjection;
use prusti_interface::environment::mir_utils::TyAsRef;
use rustc_hir::def_id::DefId;
use rustc_hir::Mutability;
use rustc_index::vec::Idx;
use rustc_middle::mir;
use rustc_middle::ty;
use rustc_middle::ty::OutlivesPredicate;
use rustc_middle::ty::ParamEnv;
use rustc_middle::ty::PredicateAtom;
use rustc_middle::ty::PredicateKind;
use rustc_middle::ty::Region;
use rustc_middle::ty::TyCtxt;

/// Contains information about which input references a given output reference can re-borrow.
#[derive(Debug, Clone)]
pub struct InterfaceReborrowingGraph<P> {
    /// This stores for every place the mutability of the corresponding reference.
    pub mutability: HashMap<P, Mutability>,
    /// This stores all places that are blocked after the function returns.
    pub blocked: HashSet<P>,
    /// This stores all places that block something after the function returns.
    pub blocking: HashSet<P>,
    /// This stores for every blocking place (e.g. _0, _0.0, _0.1, ...) the places it blocks (i.e.,
    /// may re-borrow from) after the function returns.
    pub reborrows: HashMap<P, Vec<P>>
}

impl<P: Ord> InterfaceReborrowingGraph<P> {
    pub fn blocking(&self) -> impl Iterator<Item=&P> {
        self.blocking.iter().sorted()
    }
}

impl<'a, P: Eq + Hash + 'a> InterfaceReborrowingGraph<P> {
    const NO_PLACES: &'a [P] = &[];

    /// Returns all input places that the given output place can re-borrow.
    pub fn reborrows(&self, output: &P) -> &[P] {
        if let Some(input_places) = self.reborrows.get(output) {
            input_places
        } else {
            Self::NO_PLACES
        }
    }
}

impl<'tcx> InterfaceReborrowingGraph<mir::Place<'tcx>> {
    /// Constructs the re-borrow signature for the given `def_id`. The `tymap` maps generic
    /// parameters to their actual values.
    pub fn construct(
        tcx: TyCtxt<'tcx>,
        tymap: &HashMap<ty::Ty<'tcx>, ty::Ty<'tcx>>,
        def_id: DefId
    ) -> (Self, Vec<mir::Place<'tcx>>) {
        let tymap = |ty| match tymap.get(ty) {
            Some(ty) => *ty,
            None => ty
        };

        let fn_sig = tcx.fn_sig(def_id);

        // Creates a list of items `(place, region)`, where `place` is a place passed as an
        // argument and region is the region associated with this place.
        let inputs = fn_sig.inputs().skip_binder().iter().enumerate()
            .map(|(i, ty)| (mir::Local::new(i+1), tymap(ty)))
            .filter_map(|(local, ty)|
                if let Some((region, _, mutability)) = ty.as_ty_ref() {
                    let place: mir::Place = local.into();
                    let place = place.deref(tcx);
                    Some((place, region, mutability))
                } else {
                    None
                })
            .collect::<Vec<_>>();

        // Creates a list of items `(place, region)`, where `place` is a returned place and region
        // is the region associated with this place.
        let return_place = mir::RETURN_PLACE.into();
        let return_ty = fn_sig.output().skip_binder();
        let outputs = enumerate_all_places(return_place, return_ty, tcx).into_iter()
            .map(|(place, ty)| (place, tymap(ty)))
            .filter_map(|(place, ty)|
                if let Some((region, _, mutability)) = ty.as_ty_ref() {
                    Some((place.deref(tcx), region, mutability))
                } else {
                    None
                })
            .collect::<Vec<_>>();

        // Gathers all regions that appear in input or output parameters.
        let regions = inputs.iter().chain(outputs.iter())
            .map(|(_, region, _)| *region)
            .collect::<HashSet<Region>>();

        // Gathers the mutability for all places.
        let mutability = inputs.iter().chain(outputs.iter()).cloned()
            .map(|(place, _, mutability)| (place, mutability))
            .collect::<HashMap<mir::Place<'tcx>, Mutability>>();

        // Builds the `outlives` and `outlived_by` mappings. Both are reflexive and transitive, and
        // take a region `r` to the list of all regions that outlive or are outlived by `r`.
        let outlives = gather_outlives(tcx.param_env(def_id));
        let outlives = build_transitive_outlives(&regions, &outlives);
        let outlived_by = invert_mapping(&outlives);

        let mut reborrows = HashMap::<mir::Place<'tcx>, Vec<mir::Place<'tcx>>>::new();

        // Populates `reborrows` by inserting for every output place `p` all input places `q`
        // that outlive `p`.
        for (output, output_region, _) in &outputs {
            let no_regions = HashSet::new();
            let output_region_outlived_by = outlived_by.get(output_region).unwrap_or(&no_regions);
            let output_reborrows_from = inputs.iter()
                .filter_map(|(place, input_region, _)|
                    if output_region_outlived_by.contains(input_region) {
                        Some(place.clone())
                    } else {
                        None
                    })
                .collect();
            reborrows.insert(output.clone(), output_reborrows_from);
        }

        let reborrow_signature = Self::new(mutability, reborrows)
            .map(|place| tcx.erase_all_regions(&place));

        let returned_inputs = inputs.into_iter()
            .filter(|(input, _, _)| !reborrow_signature.blocked.contains(input))
            .map(|(input, _, _)| input)
            .collect();

        (reborrow_signature, returned_inputs)
    }
}

impl<P: Clone + Eq + Hash> InterfaceReborrowingGraph<P> {
    pub fn new(
        mutability: HashMap<P, Mutability>,
        reborrows: HashMap<P, Vec<P>>
    ) -> Self {
        // Gathers all input places that are blocked after the function returns, which are simply
        // all the places that appear in a value of `reborrows`.
        let blocked = reborrows.values().flatten().cloned().collect();

        // Gathers all places that block something after the function returns, which are the keys
        // of `reborrows` that map to a non-empty list of blocked places.
        let blocking = reborrows.iter()
            .filter_map(|(blocking, blocked)|
                if !blocked.is_empty() {
                    Some(blocking.clone())
                } else {
                    None
                })
            .collect();

        InterfaceReborrowingGraph { mutability, blocked, blocking, reborrows }
    }

    /// This updates the re-borrow signature to reflect the state of the re-borrow situation after
    /// the given output has expired. Concretely, this makes the following changes:
    ///  - Removes the expired output from `self.outputs`, `self.mutability`, `self.blocking`, and
    ///    `self.reborrows`.
    ///  - Removes inputs that are not blocked currently. This has the effect that after expiring
    ///    the output, `self.returned_inputs()` returns the inputs that are freshly unblocked by
    ///    the expiration.
    pub fn expire_output(&self, output: &P) -> (Self, HashSet<P>) {
        // Check that this output reference is actually blocking something.
        assert!(self.blocking.contains(output));

        // Construct the new re-borrow signature after the expiry.
        let mut reborrows = self.reborrows.clone();
        reborrows.remove(output);
        let new_reborrow_signature = Self::new(self.mutability.clone(), reborrows);

        // The unblocked places are the ones that were blocked before the expiry but are not
        // blocked now.
        let unblocked = self.blocked.difference(&new_reborrow_signature.blocked)
            .cloned().collect();

        (new_reborrow_signature, unblocked)
    }
}

impl<P: Eq + Hash> InterfaceReborrowingGraph<P> {
    pub fn map<Q: Clone + Eq + Hash>(self,
        f: impl Fn(P) -> Q
    ) -> InterfaceReborrowingGraph<Q> {
        let mutability = self.mutability.into_iter()
            .map(|(place, mutability)| (f(place), mutability))
            .collect();
        let reborrows = self.reborrows.into_iter()
            .map(|(blocking, blocked)| (
                f(blocking),
                blocked.into_iter().map(&f).collect()
            ))
            .collect();
        InterfaceReborrowingGraph::new(mutability, reborrows)
    }

    pub fn map_with_into<Q: Clone + Eq + Hash + From<P>>(
        self
    ) -> InterfaceReborrowingGraph<Q> {
        self.map(|p| p.into())
    }
}

/// Calculates the lifetime constraints for the given `param_env`. If the result value is called
/// `o`, then `o[r]` is the set of regions outlived by `r`.
///
/// Take the following set of lifetime constraints for example:
///
/// ```ignore
/// 'a: 'b + 'c
/// 'b
/// 'c: 'b
/// 'd: 'a + 'c
/// ```
///
/// Then `o['a] = ('b, 'c)`, `o['c] = ('b)`, `o['d] = ('a, 'c)`, `o['b] = ()`.
fn gather_outlives(param_env: ParamEnv) -> HashMap<Region, HashSet<Region>> {
    let mut result = HashMap::<Region, HashSet<Region>>::new();
    let outlives = param_env.caller_bounds().iter()
        .filter_map(|b| match b.kind() {
            PredicateKind::Atom(PredicateAtom::RegionOutlives(p)) => Some(p),
            _ => None
        })
        .map(|OutlivesPredicate(a, b)| (a, b));
    for (a, b) in outlives {
        result.entry(a).or_default().insert(b);
    }
    result
}

fn build_transitive_outlives<'a, 'tcx: 'a>(
    regions: impl IntoIterator<Item=&'a Region<'tcx>>,
    outlives: &HashMap<Region<'tcx>, HashSet<Region<'tcx>>>
) -> HashMap<Region<'tcx>, HashSet<Region<'tcx>>> {
    let mut outlives_transitive = HashMap::new();
    for region in regions.into_iter() {
        build_transitive_outlives_for_region(
            region, outlives, &mut outlives_transitive);
    }
    outlives_transitive
}

fn build_transitive_outlives_for_region<'a, 'tcx: 'a>(
    region1: Region<'tcx>,
    outlives: &HashMap<Region<'tcx>, HashSet<Region<'tcx>>>,
    transitive_outlives: &'a mut HashMap<Region<'tcx>, HashSet<Region<'tcx>>>
) {
    if transitive_outlives.contains_key(region1) {
        // Regions outlived by region1 are already computed.
        return;
    }

    // Initially, the region outlives itself.
    let mut region1_transitive_outlives = HashSet::new();
    region1_transitive_outlives.insert(region1);

    let no_regions = HashSet::new();
    let region1_outlives = outlives.get(region1).unwrap_or(&no_regions);
    for region2 in region1_outlives {
        build_transitive_outlives_for_region(region2, outlives, transitive_outlives);
        let region2_transitive_outlives = transitive_outlives.get(region2).unwrap();
        for region3 in region2_transitive_outlives {
            region1_transitive_outlives.insert(region3);
        }
    }

    // Remember result for future calls.
    transitive_outlives.insert(region1, region1_transitive_outlives.clone());
}

#[test]
fn test_build_outlives_transitive() {
    use rustc_middle::ty::RegionKind;
    use rustc_middle::ty::RegionVid;

    fn mk_region(id: u32) -> RegionKind {
        RegionKind::ReVar(RegionVid::from_u32(id))
    }

    let r = (0..4).map(mk_region).collect::<Vec<_>>();
    let r = r.iter().collect::<Vec<_>>();

    let mut outlives = HashMap::<Region, HashSet<Region>>::new();

    let r0_outlives = outlives.entry(r[0]).or_default();
    r0_outlives.insert(r[1]);
    r0_outlives.insert(r[2]);

    let r2_outlives = outlives.entry(r[2]).or_default();
    r2_outlives.insert(r[1]);

    let r3_outlives = outlives.entry(r[3]).or_default();
    r3_outlives.insert(r[0]);
    r3_outlives.insert(r[2]);

    let outlives = build_transitive_outlives(&r, &outlives);

    assert_eq!(outlives[&r[0]], vec![r[0], r[1], r[2]].into_iter().collect());
    assert_eq!(outlives[&r[1]], vec![r[1]].into_iter().collect());
    assert_eq!(outlives[&r[2]], vec![r[1], r[2]].into_iter().collect());
    assert_eq!(outlives[&r[3]], vec![r[0], r[1], r[2], r[3]].into_iter().collect());
}

/// Inverts the mapping `mapping`, which takes a value `k` to values `mapping[k] = (v1, ..., vn)`.
/// The result is a mapping `inverted` (returned by this function), which takes a value `v` to the
/// list containing all values `k` such that `v` is contained in `mapping[k]`.
///
/// In practice it is used to invert the mapping `outlives`, which takes a region `a` to all
/// regions outlived by `a`. The result is a mapping `outlived_by` which takes a region `a` to all
/// regions that outlive `a`.
fn invert_mapping<T: Eq + Hash + Clone>(
    mapping: &HashMap<T, HashSet<T>>
) -> HashMap<T, HashSet<T>> {
    let mut inverted_mapping = HashMap::<T, HashSet<T>>::new();

    for (k, values) in mapping {
        for v in values {
            inverted_mapping.entry(v.clone()).or_default().insert(k.clone());
        }
    }

    inverted_mapping
}
