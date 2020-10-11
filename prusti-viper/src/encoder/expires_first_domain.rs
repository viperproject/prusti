use prusti_common::vir;

lazy_static! {
    pub static ref EXPIRES_FIRST: vir::DomainFunc = vir::DomainFunc {
        name: "expires_first".into(),
        formal_args: vec![
            vir::LocalVar::new("x", vir::Type::Int),
            vir::LocalVar::new("y", vir::Type::Int)],
        return_type: vir::Type::Bool,
        unique: false,
        domain_name: "expires_first".into()
    };

    pub static ref DOMAIN: vir::Domain = {
        let quantified_i = vir::LocalVar::new("i", vir::Type::Int);
        let quantified_j = vir::LocalVar::new("j", vir::Type::Int);
        let quantified_y = vir::LocalVar::new("y", vir::Type::Int);

        let mutually_exclusive_axiom = vir::DomainAxiom {
            name: "mutually_exclusive".into(),
            expr: vir::Expr::forall(
                vec![quantified_y.clone(), quantified_i.clone()],
                vec![vir::Trigger::new(vec![
                    vir::Expr::domain_func_app(EXPIRES_FIRST.clone(), vec![
                        vir::Expr::local(quantified_i.clone()),
                        vir::Expr::local(quantified_y.clone()),
                    ])
                ])],
                vir::Expr::forall(
                    vec![quantified_j.clone()],
                    vec![vir::Trigger::new(vec![
                        vir::Expr::domain_func_app(EXPIRES_FIRST.clone(), vec![
                            vir::Expr::local(quantified_j.clone()),
                            vir::Expr::local(quantified_y.clone()),
                        ])
                    ])],
                    vir::Expr::implies(
                        vir::Expr::ne_cmp(
                            vir::Expr::local(quantified_i.clone()),
                            vir::Expr::local(quantified_j.clone())
                        ),
                        vir::Expr::implies(
                            vir::Expr::domain_func_app(EXPIRES_FIRST.clone(), vec![
                                vir::Expr::local(quantified_i.clone()),
                                vir::Expr::local(quantified_y.clone()),
                            ]),
                            vir::Expr::not(
                                vir::Expr::domain_func_app(EXPIRES_FIRST.clone(), vec![
                                    vir::Expr::local(quantified_j.clone()),
                                    vir::Expr::local(quantified_y.clone()),
                                ])
                            )
                        )
                    )
                )
            ),
            domain_name: "expires_first".into()
        };

        vir::Domain {
            name: "expires_first".into(),
            functions: vec![EXPIRES_FIRST.clone()],
            axioms: vec![mutually_exclusive_axiom],
            type_vars: vec![]
        }
    };
}
