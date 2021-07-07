# Macros

Prusti makes use of [procedural macros](https://doc.rust-lang.org/reference/procedural-macros.html) as the interface for user-provided specifications.

## Specification syntax

Prusti [specification syntax](https://viperproject.github.io/prusti-dev/user-guide/syntax.html) is a superset of Rust boolean expressions. The parsing of Prusti-specific syntax is achieved with a custom preparser. Parsing of Rust syntax is delegated to the [`syn`](https://crates.io/crates/syn) crate.

 - [`prusti-specs/src/specifications/preparser.rs`](https://github.com/viperproject/prusti-dev/blob/f3ce1acd3c38e9c60d94fbdd7ebc1fcbeb316067/prusti-specs/src/specifications/preparser.rs)

The preparser accepts the following EBNF grammar:

```ebnf
assertion ::= prusti_expr ;
pledge ::= pledge_lhs, ",", prusti_expr ;
pledge_lhs ::= [ ? actual rust expression ?, "=>" ], prusti_expr ;
 
prusti_expr ::= conjunction, [ "==>", prusti_expr ] ;
conjunction ::= entailment, { "&&", entailment } ;
entailment ::= primary | ? actual rust expression ?, [ "|=", [ "|", ? args as parsed by syn2 ?, "|" ], "[", [ ( requires | ensures ), { ",", ( requires | ensures ) } ], "]" ] ;
primary ::= "(", prusti_expr, ")"
          | "forall", "(", "|", ? one or more args as parsed by syn2 ?, "|", prusti_expr, [ ",", "triggers", "=", ? array as parsed by syn2 ? ] ")"
          ;
requires ::= "requires", "(", prusti_expr, ")" ;
ensures ::= "ensures", "(", prusti_expr, ")" ;
```

## Specification serialization

Because procedural macros are executed in a separate, sandboxed process, it is not straight-forward to pass data from procedural macros to the rest of the [Prusti pipeline](pipeline/summary.md). Any collected data (e.g. parsed assertions) is therefore serialized and inserted into the processed HIR as attributes on dummy functions, where it can be picked up by a HIR visitor.

 - [`prusti-specs/src/rewriter.rs`](https://github.com/viperproject/prusti-dev/blob/f3ce1acd3c38e9c60d94fbdd7ebc1fcbeb316067/prusti-specs/src/rewriter.rs) - rewriter, generates functions with serialized specifications.
 - [`prusti-interface/src/specs/mod.rs` - `SpecCollector`](https://github.com/viperproject/prusti-dev/blob/f3ce1acd3c38e9c60d94fbdd7ebc1fcbeb316067/prusti-interface/src/specs/mod.rs#L41) - specification collector, implements [`intravisit::Visitor`](https://doc.rust-lang.org/nightly/nightly-rustc/rustc_hir/intravisit/trait.Visitor.html) for walking the HIR.
