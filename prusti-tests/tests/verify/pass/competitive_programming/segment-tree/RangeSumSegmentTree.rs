// compile-flags: -Puse_more_complete_exhale=false
// https://codeforces.com/problemset/problem/110/E

#![feature(box_patterns)]
#![feature(box_syntax)]

use prusti_contracts::*;
use std::ptr;

pub struct Tree {
    v: isize,
    left: Option<Box<Tree>>,
    right: Option<Box<Tree>>,
}

impl Tree {
    #[trusted]
    #[pure]
    #[ensures (result == self.v)]
    pub fn v(&self) -> isize {
        self.v
    }

    #[trusted]
    #[ensures (result.v() == vv)]
    #[ensures(is_leaf(&result))]
    pub fn new_leaf(vv: isize) -> Self {
        Tree {
            v: vv,
            left: None,
            right: None,
        }
    }
}

pub struct VecWrapperI32 {
    _ghost_size: usize,
    v: Vec<isize>,
}

impl VecWrapperI32 {
    #[trusted]
    #[pure]
    #[ensures (0 <= result)]
    pub fn len(&self) -> isize {
        self._ghost_size as isize
    }

    #[trusted]
    #[requires(size > 0)]
    #[ensures (result.len() == size)]
    #[ensures (forall(|i: isize| (0 <= i && i < result.len()) ==> result.lookup(i) == 0))]
    pub fn new(size: isize) -> Self {
        Self {
            _ghost_size: size as usize,
            v: vec![0; size as usize],
        }
    }

    #[trusted]
    #[pure]
    #[requires (0 <= index && index < self.len())]
    pub fn lookup(&self, index: isize) -> isize {
        self.v[index as usize]
    }
}

#[pure]
#[ensures(sub_size(node) == 1 ==> result)]
fn is_leaf(node: &Tree) -> bool {
    let mut result = true;

    let tL = &node.left;
    match tL {
        None => {}
        Some(box l) => {
            result = false;
        }
    }

    let tR = &node.right;
    match tR {
        None => {}
        Some(box r) => {
            result = false;
        }
    }

    result
}

#[pure]
#[ensures(result >= 1)]
fn sub_size(node: &Tree) -> isize {
    let mut sz = 1isize;

    let tL = &node.left;
    match tL {
        None => {}
        Some(box l) => {
            sz += sub_size(l);
        }
    }

    let tR = &node.right;
    match tR {
        None => {}
        Some(box r) => {
            sz += sub_size(r);
        }
    }
    sz
}

#[pure]
fn leaves_count(node: &Tree) -> isize {
    if is_leaf(node) {
        1
    } else {
        let mut result = 0;
        let tL = &node.left;
        match tL {
            None => {}
            Some(box l) => {
                result += leaves_count(l);
            }
        }

        let tR = &node.right;
        match tR {
            None => {}
            Some(box r) => {
                result += leaves_count(r);
            }
        }
        result
    }
}

#[pure]
fn is_complete(node: &Tree) -> bool {
    let mut result = true;

    let mut sizeL = 0;
    let mut leavesL = 0;
    let tL = &node.left;
    match tL {
        None => {}
        Some(box l) => {
            result &= is_complete(l);
            sizeL = sub_size(l);
            leavesL = leaves_count(l);
        }
    }

    let tR = &node.right;
    match tR {
        None => {
            result &= sizeL == 0;
        }
        Some(box r) => {
            result &= sub_size(r) == sizeL;
            result &= is_complete(r);
            result &= leaves_count(r) == leavesL;
        }
    }

    result
}

#[pure]
#[requires(is_complete(node))]
#[ensures(result)]
fn ensures_semmetry(node: &Tree) -> bool {
    let mut result = true;
    let size = sub_size(node);
    let leaves = leaves_count(node);

    let mut sizeL = 0;
    let mut leavesL = 0;
    let tL = &node.left;
    match tL {
        None => {}
        Some(box l) => {
            sizeL = sub_size(l);
            leavesL = leaves_count(l);
        }
    }

    let tR = &node.right;
    let mut sizeR = 0;
    let mut leavesR = 0;
    match tR {
        None => {}
        Some(box r) => {
            sizeR = sub_size(r);
            leavesR = leaves_count(r);
        }
    }

    assert!(sizeL == sizeR);
    assert!(leavesL == leavesR);

    let childrenSize = size - 1;
    let childSize = childrenSize / 2;
    assert!(childrenSize == childSize * 2);

    let tL = &node.left;
    match tL {
        None => {}
        Some(box l) => {
            result &= sub_size(l) == childSize;
            let childLeaves = leaves / 2;
            assert!(leaves == childLeaves * 2);
            result &= leaves_count(l) == childLeaves;
        }
    }

    let tR = &node.right;
    let mut sizeR = 0;
    match tR {
        None => {}
        Some(box r) => {
            result &= sub_size(r) == childSize;
            let childLeaves = leaves / 2;
            assert!(leaves == childLeaves * 2);
            result &= leaves_count(r) == childLeaves;
        }
    }

    result
}

#[pure]
fn sum_property(node: &Tree) -> bool {
    if is_leaf(node) {
        true
    } else {
        let mut leaves = 0;
        let mut result = true;
        let tL = &node.left;
        match tL {
            None => {}
            Some(box l) => {
                result &= sum_property(l);
                leaves += l.v();
            }
        }

        let tR = &node.right;
        match tR {
            None => {}
            Some(box r) => {
                result &= sum_property(r);
                leaves += r.v();
            }
        }
        result && leaves == node.v()
    }
}

#[pure]
#[ensures(result <= a && result <= b)]
#[ensures(result == a || result == b)]
fn min(a: isize, b: isize) -> isize {
    if a < b {
        a
    } else {
        b
    }
}

#[pure]
#[ensures(result >= a && result >= b)]
#[ensures(result == a || result == b)]
fn max(a: isize, b: isize) -> isize {
    if a > b {
        a
    } else {
        b
    }
}

#[pure]
#[requires(gLIdx >= 0 && gRIdx <= array.len() && gLIdx < gRIdx)]
#[requires(gRIdx - gLIdx == rIdx - lIdx)]
#[requires(nodeLIdx >= 0 && nodeRIdx <= array.len() && nodeLIdx < nodeRIdx)]
#[requires(power_of_two(nodeRIdx - nodeLIdx))]
#[requires(gLIdx >= nodeLIdx && gRIdx <= nodeRIdx)]
#[requires(is_complete(node))]
#[requires(lIdx >= 0 && lIdx < rIdx && rIdx <= leaves_count(node))]
#[requires(nodeRIdx - gRIdx == leavesCount - rIdx)]
#[requires(ensures_semmetry(node))]
#[requires(leavesCount == leaves_count(node))]
#[requires(sum_property(node))]
#[requires(leaves_count(node) == nodeRIdx - nodeLIdx)]
#[requires(array_tree_equivelant(node, array, nodeLIdx, nodeRIdx))]
#[ensures((lIdx == 0 && rIdx == leavesCount) ==> result == node.v())]
#[ensures(result == array_range_sum(array, gLIdx, gRIdx))]
fn sum_of_leaves(
    node: &Tree,
    lIdx: isize,
    rIdx: isize,
    leavesCount: isize,
    gLIdx: isize,
    gRIdx: isize,
    array: &VecWrapperI32,
    nodeLIdx: isize,
    nodeRIdx: isize,
) -> isize {
    if is_leaf(node) {
        assert!(lIdx == 0 && rIdx == leavesCount);
        assert!(node.v() == array.lookup(gLIdx));
        assert!(gLIdx == gRIdx - 1);
        assert!(node.v() == array_range_sum(array, gLIdx, gRIdx));
        node.v()
    } else {

        let midIdx = leavesCount / 2;
        assert!(leaves_count(node) == midIdx * 2);

        assert!((nodeRIdx + nodeLIdx) % 2 == 0);
        let mid = (nodeRIdx + nodeLIdx) / 2;

        let mut resultL = 0;
        let tL = &node.left;
        match tL {
            None => {}
            Some(box l) => {
                if lIdx < midIdx {
                    resultL = sum_of_leaves(
                        l,
                        lIdx,
                        min(rIdx, midIdx),
                        midIdx,
                        gLIdx,
                        min(mid, gRIdx),
                        array,
                        nodeLIdx,
                        mid,
                    );
                    assert!(rIdx - midIdx == gRIdx - mid);
                } else {
                }
            }
        }

        let _t = tL;

        let mut resultR = 0;
        let tR = &node.right;
        match tR {
            None => {}
            Some(box r) => {
                if rIdx > midIdx {
                    resultR = sum_of_leaves(
                        r,
                        max(midIdx, lIdx) - midIdx,
                        rIdx - midIdx,
                        midIdx,
                        max(mid, gLIdx),
                        gRIdx,
                        array,
                        mid,
                        nodeRIdx,
                    );
                } else {
                }
            }
        }

        
        if lIdx < midIdx && rIdx > midIdx  {
            assert!(resultL == array_range_sum(array, gLIdx,  mid));
            assert!(resultR == array_range_sum(array, mid,  gRIdx));
        } else if lIdx < midIdx{
            assert!(resultL == array_range_sum(array, gLIdx,  gRIdx));
            assert!(resultR == 0);
        }else if rIdx > midIdx{
            assert!(resultR == array_range_sum(array, gLIdx,  gRIdx));
            assert!(resultL == 0);
        } else {
            assert!(false);
        }

        let _t = tR;

        let result = resultL + resultR;

        assert!(result == array_range_sum(array, gLIdx, gRIdx));
        result
    }
}

#[pure]
#[requires(gLIdx >= 0 && gRIdx <= array.len() && gLIdx < gRIdx)]
#[requires(gRIdx - gLIdx == rIdx - lIdx)]
#[requires(nodeLIdx >= 0 && nodeRIdx <= array.len() && nodeLIdx < nodeRIdx)]
#[requires(power_of_two(nodeRIdx - nodeLIdx))]
#[requires(gLIdx >= nodeLIdx && gRIdx <= nodeRIdx)]
#[requires(is_complete(node))]
#[requires(lIdx >= 0 && lIdx < rIdx && rIdx <= leaves_count(node))]
#[requires(nodeRIdx - gRIdx == leavesCount - rIdx)]
#[requires(ensures_semmetry(node))]
#[requires(leavesCount == leaves_count(node))]
#[requires(sum_property(node))]
#[requires(leaves_count(node) == nodeRIdx - nodeLIdx)]
#[requires(array_tree_equivelant(node, array, nodeLIdx, nodeRIdx))]
#[ensures(result == sum_of_leaves(node, lIdx, rIdx, leavesCount, gLIdx, gRIdx, array, nodeLIdx, nodeRIdx))]
#[ensures(result == array_range_sum(array, gLIdx, gRIdx))]
fn range_sum(
    node: &Tree,
    lIdx: isize,
    rIdx: isize,
    leavesCount: isize,
    gLIdx: isize,
    gRIdx: isize,
    array: &VecWrapperI32,
    nodeLIdx: isize,
    nodeRIdx: isize,
) -> isize {
    if lIdx == 0 && rIdx == leavesCount {
        node.v()
    } else {
        let mut result = 0;

        let midIdx = leavesCount / 2;
        assert!(leaves_count(node) == midIdx * 2);

        assert!((nodeRIdx + nodeLIdx) % 2 == 0);
        let mid = (nodeRIdx + nodeLIdx) / 2;
        assert!(mid < nodeRIdx);

        let tL = &node.left;
        match tL {
            None => {}
            Some(box l) => {
                if lIdx < midIdx {
                    result += range_sum(
                        l,
                        lIdx,
                        min(rIdx, midIdx),
                        midIdx,
                        gLIdx,
                        min(mid, gRIdx),
                        array,
                        nodeLIdx,
                        mid,
                    );
                } else {
                    result += 0;
                }
            }
        }

        let tR = &node.right;
        match tR {
            None => {}
            Some(box r) => {
                if rIdx > midIdx {
                    assert!(rIdx - midIdx == gRIdx - mid);
                    result += range_sum(
                        r,
                        max(midIdx, lIdx) - midIdx,
                        rIdx - midIdx,
                        midIdx,
                        max(mid, gLIdx),
                        gRIdx,
                        array,
                        mid,
                        nodeRIdx,
                    );
                } else {
                    result += 0;
                }
            }
        }
        result
    }
}

#[requires(is_complete(node))]
#[requires(ensures_semmetry(node))]
#[ensures(sub_size(node) == 2 * leaves_count(node) - 1)]
fn lemma(node: &Tree) {
    if is_leaf(node) {
    } else {
        let mut sizeL = 0;
        let mut leavesL = 0;
        let tL = &node.left;
        match tL {
            None => {}
            Some(box l) => {
                lemma(l);
            }
        }
    }
}

#[pure]
#[requires(lIdx >= 0 && rIdx <= array.len() && lIdx < rIdx)]
#[requires(power_of_two(rIdx - lIdx))]
#[requires(is_complete(node))]
#[requires(ensures_semmetry(node))]
#[requires(leaves_count(node) == rIdx - lIdx)]
fn array_tree_equivelant(node: &Tree, array: &VecWrapperI32, lIdx: isize, rIdx: isize) -> bool {
    if is_leaf(node) {
        node.v() == array.lookup(lIdx)
    } else {
        let mut result = true;
        assert!((rIdx + lIdx) % 2 == 0);
        let mid = (rIdx + lIdx) / 2;
        let tL = &node.left;
        match tL {
            None => {}
            Some(box l) => {
                result &= array_tree_equivelant(l, array, lIdx, mid);
            }
        }

        let tR = &node.right;
        match tR {
            None => {}
            Some(box r) => {
                result &= array_tree_equivelant(r, array, mid, rIdx);
            }
        }

        result
    }
}

#[requires(lIdx >= 0 && rIdx <= array.len() && lIdx < rIdx)]
#[requires(power_of_two(rIdx - lIdx))]
#[ensures(is_complete(&result))]
#[ensures(ensures_semmetry(&result))]
#[ensures(sum_property(&result))]
#[ensures(leaves_count(&result) == rIdx - lIdx)]
#[ensures(array_tree_equivelant(&result, array, lIdx, rIdx))]
fn build(array: &VecWrapperI32, lIdx: isize, rIdx: isize) -> Tree {
    if lIdx == rIdx - 1 {
        let result = Tree::new_leaf(array.lookup(lIdx));
        assert!(is_leaf(&result));
        assert!(sub_size(&result) == 1);
        result
    } else {
        assert!((rIdx + lIdx) % 2 == 0);
        let mid = (rIdx + lIdx) / 2;
        let left = build(array, lIdx, mid);
        let right = build(array, mid, rIdx);
        assert!(leaves_count(&left) == leaves_count(&right));
        lemma(&left);
        lemma(&right);
        assert!(sub_size(&left) == sub_size(&right));
        let mut result = Tree::new_leaf(left.v() + right.v());
        result.left = Some(box left);
        result.right = Some(box right);
        result
    }
}

#[pure]
fn power_of_two(v: isize) -> bool {
    if v == 1 {
        true
    } else {
        let even = v % 2 == 0;
        even && power_of_two(v / 2)
    }
}

#[pure]
#[requires(lIdx >= 0 && rIdx <= array.len() && lIdx < rIdx)]
#[ensures(lIdx == rIdx - 1 ==> result == array.lookup(lIdx))]
#[ensures(forall(|i: isize| (i > lIdx && i < rIdx) ==> result == array_range_sum(array, lIdx, i) + array_range_sum(array, i, rIdx)))]
fn array_range_sum(array: &VecWrapperI32, lIdx: isize, rIdx: isize) -> isize {
    if lIdx == rIdx - 1 {
        array.lookup(lIdx)
    } else {
        array.lookup(lIdx) + array_range_sum(array, lIdx + 1, rIdx)
    }
}

#[requires(power_of_two(array.len()))]
#[requires(lIdx >= 0 && rIdx <= array.len() && lIdx < rIdx)]
#[ensures(result == array_range_sum(array, lIdx, rIdx))]
fn solve(array: &VecWrapperI32, lIdx: isize, rIdx: isize) -> isize {
    let segTree = build(array, 0, array.len());
    range_sum(&segTree, lIdx, rIdx,  array.len(), lIdx, rIdx, array, 0, array.len())
}

fn main() {}
