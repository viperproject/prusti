# Prusti-utils

Common utils used accross the Prusti project that depend on neither
JVM nor rustc crates.

The difference between this crate and prusti-common is that
prusti-common dynamically links to the JVM, whereas prusti-utils does
not. This allows using prusti-utils from e.g. the parser and proc-macro
implementation parts of prusti.
