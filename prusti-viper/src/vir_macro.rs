macro_rules! vir_type {
    (Int) => {::prusti_common::vir::Type::Int};
    (Bool) => {::prusti_common::vir::Type::Bool};
}

macro_rules! vir_local {
    ($name: ident : $type: ident) => {
        ::prusti_common::vir::LocalVar {
            name: stringify!($name).to_string(),
            typ: vir_type!($type)
        }
    }
}

macro_rules! vir {
    (# $comment: expr) => {
        ::prusti_common::vir::Stmt::Comment($comment.to_string())
    };
    (label $label: expr) => {
        ::prusti_common::vir::Stmt::Label($label.to_string())
    };
    (assert $exp: tt) => {
        ::prusti_common::vir::Stmt::Assert(
            vir!($exp),
            ::prusti_common::vir::FoldingBehaviour::Expr,
            ::prusti_common::vir::Position::default())
    };
    (inhale $exp: tt) => {
        ::prusti_common::vir::Stmt::Inhale(
            vir!($exp),
            ::prusti_common::vir::FoldingBehaviour::Expr)
    };
    (apply $exp: tt) => {
        ::prusti_common::vir::Stmt::ApplyMagicWand(
            vir!($exp),
            ::prusti_common::vir::Position::default())
    };
    (obtain $exp: tt) => {
        ::prusti_common::vir::Stmt::Obtain(
            vir!($exp),
            ::prusti_common::vir::Position::default())
    };
    (if ($exp: tt) { $($then: tt);* } else { $($elze: tt);* }) => {
        ::prusti_common::vir::Stmt::If(vir!($exp), vec![$(vir!($then)),*], vec![$(vir!($elze)),*])
    };
    (if ($exp: tt) { $($then: tt);* }) => {
        ::prusti_common::vir::Stmt::If(vir!($exp), vec![$(vir!($then)),*], vec![])
    };
    (true) => {
        ::prusti_common::vir::Expr::Const(
            ::prusti_common::vir::Const::Bool(true),
            ::prusti_common::vir::Position::default())
    };
    (false) => {
        ::prusti_common::vir::Expr::Const(
            ::prusti_common::vir::Const::Bool(false),
            ::prusti_common::vir::Position::default())
    };
    ($lhs: tt == $rhs: tt) => {
        ::prusti_common::vir::Expr::BinOp(vir::BinOpKind::Equals,
            Box::new(vir!($lhs)),
            Box::new(vir!($rhs)),
            ::prusti_common::vir::Position::default())
    };
    ($head: tt && $tail: tt) => {
        ::prusti_common::vir::Expr::BinOp(vir::BinOpKind::And,
            Box::new(vir!($head)),
            Box::new(vir!($tail)),
            ::prusti_common::vir::Position::default())
    };
    ($head: tt || $tail: tt) => {
        ::prusti_common::vir::Expr::BinOp(vir::BinOpKind::Or,
            Box::new(vir!($head)),
            Box::new(vir!($tail)),
            ::prusti_common::vir::Position::default())
    };
    ($antecedent: tt ==> $consequent: tt) => {
        ::prusti_common::vir::Expr::BinOp(vir::BinOpKind::Implies,
            Box::new(vir!($antecedent)),
            Box::new(vir!($consequent)),
            ::prusti_common::vir::Position::default())
    };
    ($lhs: tt {$borrow: expr} --* $rhs: tt) => {
        ::prusti_common::vir::Expr::magic_wand(vir!($lhs), vir!($rhs), $borrow)
    };
    (forall $($name: ident : $type: ident),+ :: {$trigger: tt} $body: tt) => {
        ::prusti_common::vir::Expr::ForAll(
            vec![$(vir_local!($name: $type)),+],
            vec![::prusti_common::vir::Trigger::new(vec![vir!($trigger)])],
            Box::new(vir!($body)),
            ::prusti_common::vir::Position::default())
    };
    ([ $e: expr ]) => { $e.clone() };
    (( $($tokens: tt)+ )) => { vir!($($tokens)+) }
}
