use std::collections::HashSet;

use prusti_common::{vir::{self, ExprWalker, StmtWalker, WithIdentifier, compute_identifier}, vir_local};

use super::Encoder;

pub(super) fn collect_definitions(
    encoder: &Encoder,
    name: String,
    methods: Vec<vir::CfgMethod>
) -> vir::Program {

    let mut collector = Collector {
        encoder,
        used_methods: Default::default(),
        used_predicates: Default::default(),
        used_fields: Default::default(),
        used_domains: Default::default(),
        used_functions: Default::default(),
    };
    vir::utils::walk_methods(&methods, &mut collector);
    vir::Program {
        name,
        domains: Vec::new(),
        fields: collector.get_used_fields(),
        builtin_methods: collector.get_used_methods(),
        methods,
        functions: collector.get_used_functions(),
        viper_predicates: collector.get_used_predicates(),
    }
}

struct Collector<'p, 'v: 'p, 'tcx: 'v> {
    encoder: &'p Encoder<'v, 'tcx>,
    used_methods: HashSet<String>,
    used_predicates: HashSet<String>,
    used_fields: HashSet<vir::Field>,
    used_domains: HashSet<String>,
    used_functions: HashSet<vir::FunctionIdentifier>,
}

impl<'p, 'v: 'p, 'tcx: 'v> Collector<'p, 'v, 'tcx> {
    fn get_used_fields(&self) -> Vec<vir::Field> {
        self.used_fields.iter().cloned().collect()
    }
    fn get_used_methods(&self) -> Vec<vir::BodylessMethod> {
        self.encoder.get_builtin_methods().values().filter(|method| {
            self.used_methods.contains(&method.name)
        }).cloned().collect()
    }
    fn get_used_predicates(&self) -> Vec<vir::Predicate> {
        let mut predicates: Vec<_> = self.used_predicates.iter().map(|name| {
            self.encoder.get_viper_predicate(name)
        }).chain(Some(vir::Predicate::Bodyless(
            "DeadBorrowToken$".to_string(),
            vir_local!{ borrow: Int },
        ))).collect();
        predicates.sort_by_key(|f| f.get_identifier());
        predicates
    }
    fn get_used_functions(&self) -> Vec<vir::Function> {
        let mut functions: Vec<_> = self.used_functions.iter().map(|identifier| {
            self.encoder.get_function(&identifier).clone()
        }).collect();
        functions.sort_by_key(|f| f.get_identifier());
        functions
    }
}

impl<'p, 'v: 'p, 'tcx: 'v> StmtWalker for Collector<'p, 'v, 'tcx> {
    fn walk_expr(&mut self, expr: &vir::Expr) {
        ExprWalker::walk(self, expr);
    }
    fn walk_method_call(&mut self, method_name: &str, args: &Vec<vir::Expr>, targets: &Vec<vir::LocalVar>) {
        eprintln!("method_name={}", method_name);
        self.used_methods.insert(method_name.to_string());
        for arg in args {
            self.walk_expr(arg);
        }
        for target in targets {
            StmtWalker::walk_local_var(self, target);
        }
    }

    fn walk_fold(
        &mut self,
        predicate_name: &str,
        args: &Vec<vir::Expr>,
        _perm: &vir::PermAmount,
        _variant: &vir::MaybeEnumVariantIndex,
        _pos: &vir::Position
    ) {
        self.used_predicates.insert(predicate_name.to_string());
        for arg in args {
            self.walk_expr(arg);
        }
    }

    fn walk_unfold(
        &mut self,
        predicate_name: &str,
        args: &Vec<vir::Expr>,
        _perm: &vir::PermAmount,
        _variant: &vir::MaybeEnumVariantIndex,
    ) {
        self.used_predicates.insert(predicate_name.to_string());
        for arg in args {
            self.walk_expr(arg);
        }
    }

}

impl<'p, 'v: 'p, 'tcx: 'v> ExprWalker for Collector<'p, 'v, 'tcx> {
    fn walk_variant(&mut self, base: &vir::Expr, variant: &vir::Field, _pos: &vir::Position) {
        self.used_fields.insert(variant.clone());
        ExprWalker::walk(self, base);
    }
    fn walk_field(&mut self, receiver: &vir::Expr, field: &vir::Field, _pos: &vir::Position) {
        self.used_fields.insert(field.clone());
        ExprWalker::walk(self, receiver);
    }
    fn walk_predicate_access_predicate(
        &mut self,
        name: &str,
        arg: &vir::Expr,
        _perm_amount: vir::PermAmount,
        _pos: &vir::Position
    ) {
        self.used_predicates.insert(name.to_string());
        ExprWalker::walk(self, arg)
    }
    fn walk_unfolding(
        &mut self,
        name: &str,
        args: &Vec<vir::Expr>,
        body: &vir::Expr,
        _perm: vir::PermAmount,
        _variant: &vir::MaybeEnumVariantIndex,
        _pos: &vir::Position
    ) {
        self.used_predicates.insert(name.to_string());
        for arg in args {
            ExprWalker::walk(self, arg);
        }
        ExprWalker::walk(self, body);
    }
    fn walk_func_app(
        &mut self,
        name: &str,
        args: &Vec<vir::Expr>,
        formal_args: &Vec<vir::LocalVar>,
        return_type: &vir::Type,
        _pos: &vir::Position
    ) {
        let identifier: vir::FunctionIdentifier = compute_identifier(name, formal_args, return_type).into();
        if !self.used_functions.contains(&identifier) {
            let function = self.encoder.get_function(&identifier);
            self.used_functions.insert(identifier);
            for expr in function.pres.iter().chain(&function.posts).chain(&function.body) {
                self.walk_expr(expr);
            }
        }
        for arg in args {
            ExprWalker::walk(self, arg)
        }
        for arg in formal_args {
            ExprWalker::walk_local_var(self, arg);
        }
    }
    fn walk_domain_func_app(&mut self, func: &vir::DomainFunc, args: &Vec<vir::Expr>, _pos: &vir::Position) {
        self.used_domains.insert(func.domain_name.clone());
        for arg in args {
            ExprWalker::walk(self, arg)
        }
        for arg in &func.formal_args {
            ExprWalker::walk_local_var(self, arg)
        }
    }
    // fn walk_snap_app(&mut self, expr: &vir::Expr, _pos: &vir::Position) {
    //     ExprWalker::walk(self, expr);
    // }
}