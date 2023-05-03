use crate::nodes::*;

pub fn migrate<'hir>(expr: Expr<'hir>) -> Option<String> {
    let closure = expr.closure()?;
    let param = closure.single_param()?;

    // Check that `b` in `|b| ...` is a serenity builder
    let mut param_type = closure.single_param()?.type_().ref_()?;
    let [crate_, module, .., builder_type] = &*param_type.adt()?.path().parts() else { return None };
    if !(crate_ == "serenity" && module == "builder") {
        return None;
    }

    let mut body = closure.body();
    if let Some(inner) = body.single_expr_block() {
        body = inner;
    }

    let mut receiver = body;
    let mut method_calls = Vec::new();
    while let Some(method_call) = receiver.method_call() {
        receiver = method_call.receiver();
        method_calls.insert(0, method_call);
    }

    let mut replacement = format!("{}::new()", builder_type);
    for call in method_calls {
        replacement += &format!(".{}(", call.method_name());
        for arg in call.args() {
            replacement += &format!("{}, ", arg.source_code().trim());
        }
        replacement += ")";
    }

    Some(replacement)
}
