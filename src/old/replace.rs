//! Provides functions to transform our structures into serenity 0.12 compatible builders

use rustc_lint::LintContext as _;

use super::structures::*;

// fn get_line_indent(
//     source: &rustc_span::source_map::SourceMap,
//     syntax_ctxt: rustc_span::SyntaxContext,
//     cursor: rustc_span::BytePos,
// ) -> usize {
//     let Ok(line) = source.lookup_line(cursor) else { return 0 };
//     let start_of_line = line.sf.lines(|lines| lines[line.line]);

//     let Some(line_up_to_cursor) = span_to_source(
//         source,
//         syntax_ctxt,
//         rustc_span::Span::new(start_of_line, cursor, syntax_ctxt, None),
//     ) else { return 0 };
//     line_up_to_cursor
//         .find(|c: char| !c.is_whitespace())
//         .unwrap_or(0)
// }

fn span_to_source(
    source: &rustc_span::source_map::SourceMap,
    syntax_ctxt: rustc_span::SyntaxContext,
    span: rustc_span::Span,
) -> String {
    source
        .span_to_snippet(rustc_span::hygiene::walk_chain(span, syntax_ctxt))
        .unwrap_or_else(|_| "todo!()".to_owned())
}

fn field_arg_string(
    cx: &rustc_lint::LateContext<'_>,
    syntax_ctxt: rustc_span::SyntaxContext,
    args: &[BuilderCallArg],
) -> String {
    args.iter()
        .map(|arg| match arg {
            BuilderCallArg::Literal(expr) => {
                span_to_source(cx.sess().source_map(), syntax_ctxt, expr.span)
            }
            BuilderCallArg::NestedClosure(closure) => replace_closure(cx, closure),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn replace_builder_call_chain_stmt(
    cx: &rustc_lint::LateContext<'_>,
    syntax_ctxt: rustc_span::SyntaxContext,
    call_chain: &BuilderCallChain,
) -> String {
    let builder_ident = call_chain.receiver;
    let mut line = format!("{builder_ident} = {builder_ident}");
    for BuilderCall { field, args } in &call_chain.calls {
        let arg_string = field_arg_string(cx, syntax_ctxt, &args);
        line += &format!(".{field}({arg_string})");
    }
    line += ";";
    line
}

fn last_path_segment(expr: &rustc_hir::Expr<'_>) -> Option<rustc_span::symbol::Ident> {
    let rustc_hir::ExprKind::Path(qpath) = &expr.kind else { return None };
    let rustc_hir::QPath::Resolved(None, path) = qpath else { return None };
    Some(path.segments.last()?.ident)
}

fn replace_create_interaction_response(
    cx: &rustc_lint::LateContext<'_>,
    mut closure: &BuilderClosure,
) -> String {
    let syntax_ctxt = closure.span.ctxt();

    // Variants must be named exactly like serenity 0.12 CreateInteractionResponse variants
    #[derive(Debug)]
    enum Variant {
        Pong,
        Message,
        Defer,
        Acknowledge,
        UpdateMessage,
        Autocomplete,
        Modal,
    }

    // Find response type
    let mut kind = String::new(); // empty string triggers the fallback case below
    if let Some(call) = closure.call_chain.calls.iter().find(|call| call.field.as_str() == "kind") {
        if let [BuilderCallArg::Literal(explicit_kind)] = &*call.args {
            if let Some(explicit_kind) = last_path_segment(explicit_kind) {
                kind = explicit_kind.as_str().to_owned();
            }
        }
    }
    let (new_variant, inner_builder) = match &*kind {
        "Pong" => ("Pong", None),
        "DeferredChannelMessageWithSource" => ("Defer", Some("CreateInteractionResponseMessage")),
        "DeferredUpdateMessage" => ("Acknowledge", None),
        "UpdateMessage" => ("UpdateMessage", Some("CreateInteractionResponseMessage")),
        "Autocomplete" => ("Autocomplete", Some("CreateAutocompleteResponse")),
        "Modal" => ("Modal", Some("CreateModal")),
        // this was default in 0.11
        "ChannelMessageWithSource" | _ => ("Message", Some("CreateInteractionResponseMessage")),
    };

    // Find response data
    let mut fields: &[BuilderCall<'_>] = &[];
    if let Some(call) = closure
        .call_chain
        .calls
        .iter()
        .find(|call| call.field.as_str() == "interaction_response_data")
    {
        if let Some(BuilderCallArg::NestedClosure(closure)) = call.args.iter().next() {
            assert!(closure.stmts.is_empty()); // Ignoring stmts for now
            fields = &*closure.call_chain.calls;
        }
    }

    let mut output = format!("CreateInteractionResponse::{}", new_variant);
    if let Some(inner_builder) = inner_builder {
        output += "(CreateInteractionResponseMessage::new()";
        for BuilderCall { field, args } in fields {
            let arg_string = field_arg_string(cx, syntax_ctxt, args);
            output += &format!("\n.{field}({arg_string})");
        }
        output += "\n)";
    }
    output
}

fn replace_button(cx: &rustc_lint::LateContext<'_>, mut closure: &BuilderClosure) -> String {
    let syntax_ctxt = closure.span.ctxt();

    let mut url = None;
    let mut custom_id = None;
    let mut optional_args = Vec::new();
    for call in &closure.call_chain.calls {
        let Some(BuilderCallArg::Literal(value)) = call.args.iter().next() else { panic!() };
        match call.field.as_str() {
            "url" => url = Some(span_to_source(cx.sess().source_map(), syntax_ctxt, value.span)),
            "custom_id" => {
                custom_id = Some(span_to_source(cx.sess().source_map(), syntax_ctxt, value.span))
            }
            other => optional_args
                .push((other, span_to_source(cx.sess().source_map(), syntax_ctxt, value.span))),
        }
    }

    let mut replacement = if let Some(url) = url {
        format!("CreateButton::new_link({url})")
    } else {
        let custom_id = custom_id.unwrap();
        format!("CreateButton::new({custom_id})")
    };
    for (field, value) in &optional_args {
        replacement += &format!("\n.{field}({value})");
    }
    replacement
}

fn replace_select_menu(cx: &rustc_lint::LateContext<'_>, mut closure: &BuilderClosure) -> String {
    let syntax_ctxt = closure.span.ctxt();

    let mut custom_id = None;
    let mut options = None;
    let mut optional_args = Vec::new();
    for call in &closure.call_chain.calls {
        match call.field.as_str() {
            "custom_id" => {
                let Some(BuilderCallArg::Literal(custom_id_expr)) = call.args.iter().next() else { panic!() };

                custom_id =
                    Some(span_to_source(cx.sess().source_map(), syntax_ctxt, custom_id_expr.span));
            }
            "options" => {
                let Some(BuilderCallArg::NestedClosure(options_closure)) = call.args.iter().next() else { panic!() };
                let mut option_replacements = Vec::new();
                for call in &options_closure.call_chain.calls {
                    option_replacements.push(match call.field.as_str() {
                        "create_option" =>  {
                            let Some(BuilderCallArg::NestedClosure(option)) = call.args.iter().next() else { panic!() };
                            replace_generic(cx, option)
                        },
                        other => unimplemented!("{}", other),
                    });
                }

                options = Some(option_replacements.join(",\n"));
            }
            other => {
                let Some(BuilderCallArg::Literal(value)) = call.args.iter().next() else { panic!() };
                optional_args
                    .push((other, span_to_source(cx.sess().source_map(), syntax_ctxt, value.span)))
            }
        }
    }

    let custom_id = custom_id.unwrap();
    let options = options.unwrap();
    format!(
        "CreateSelectMenu::new({custom_id}, CreateSelectMenuKind::String {{ options: vec![\n{options}\n] }})"
    )
}

fn replace_create_components(
    cx: &rustc_lint::LateContext<'_>,
    mut closure: &BuilderClosure,
) -> String {
    let syntax_ctxt = closure.span.ctxt();

    let mut rows = Vec::new();
    for call in &closure.call_chain.calls {
        if call.field.as_str() != "create_action_row" {
            panic!("unknown field {}", call.field);
        }
        let Some(BuilderCallArg::NestedClosure(row)) = call.args.first() else { panic!() };

        let mut buttons = Vec::new();
        let mut select_menu = None;
        let mut input_text = None;
        for component in &row.call_chain.calls {
            match component.field.as_str() {
                "create_button" => {
                    let Some(BuilderCallArg::NestedClosure(closure)) = component.args.iter().next() else { panic!() };
                    buttons.push(replace_button(cx, closure));
                }
                "add_button" => {
                    let Some(BuilderCallArg::Literal(builder)) = component.args.iter().next() else { panic!() };
                    buttons.push(span_to_source(cx.sess().source_map(), syntax_ctxt, builder.span));
                }
                "create_select_menu" => {
                    let Some(BuilderCallArg::NestedClosure(closure)) = component.args.iter().next() else { panic!() };
                    select_menu = Some(replace_select_menu(cx, closure));
                }
                "add_select_menu" => {
                    let Some(BuilderCallArg::Literal(builder)) = component.args.iter().next() else { panic!() };
                    select_menu =
                        Some(span_to_source(cx.sess().source_map(), syntax_ctxt, builder.span));
                }
                "create_input_text" => {
                    input_text = Some("unimplemented!()");
                    panic!();
                }
                "add_input_text" => {
                    input_text = Some("unimplemented!()");
                    panic!();
                }
                other => panic!("unknown {}", other),
            }
        }

        rows.push(if !buttons.is_empty() {
            format!("CreateActionRow::Buttons(vec![{}])", buttons.join(",\n"))
        } else if let Some(select_menu) = select_menu {
            format!("CreateActionRow::SelectMenu({})", select_menu)
        } else {
            let input_text = input_text.unwrap();
            format!("CreateActionRow::InputText({})", input_text)
        });
    }

    format!("vec![{}]", rows.join(",\n"))
}

fn replace_generic(cx: &rustc_lint::LateContext<'_>, mut closure: &BuilderClosure) -> String {
    let syntax_ctxt = closure.span.ctxt();

    let required_fields: &[&str] = match &*closure.builder_type {
        "AddMember" => &["access_token"],
        "CreateApplicationCommand" => &["name"],
        "CreateChannel" => &["name"],
        "CreateButton" => &["custom_id"],
        "CreateSelectMenu" => &["custom_id", "kind"],
        "CreateSelectMenuOption" => &["label", "value"],
        "CreateEmbedAuthor" => &["name"],
        "CreateEmbedFooter" => &["text"],
        "CreateModal" => &["custom_id", "title"],
        "CreateStageInstance" => &["channel_id", "topic"],
        "CreateThread" => &["name"],
        "CreateWebhook" => &["name"],
        "CreateQuickModal" => &["title"],
        "CreateCommandOption" => &["kind", "name", "description"],
        "CreateInputText" => &["style", "label", "custom_id"],
        "CreateScheduledEvent" => &["kind", "name", "scheduled_start_time"],
        "CreateSticker" => &["name", "tags", "description", "file"],
        _ => &[],
    };

    let mut required_args = Vec::new();
    let mut optional_args = Vec::new();
    for call in &closure.call_chain.calls {
        if required_fields.contains(&call.field.as_str()) {
            required_args.push(field_arg_string(cx, syntax_ctxt, &call.args));
        } else {
            optional_args.push((call.field, field_arg_string(cx, syntax_ctxt, &call.args)));
        }
    }

    let binding = &closure.binding;
    let ty = &closure.builder_type;
    let required_args = required_args.join(", ");
    if closure.stmts.is_empty() {
        let mut output = format!("{ty}::new({required_args})");
        for (field, args) in optional_args {
            output += &format!("\n.{field}({args})");
        }
        output
    } else {
        let mut stmts_string = String::new();
        for stmt in &closure.stmts {
            stmts_string += &span_to_source(cx.sess().source_map(), syntax_ctxt, stmt.span);
            stmts_string += "\n";
        }

        let mut output =
            format!("{{\nlet mut {binding} = {ty}::new({required_args});\n{stmts_string}{binding}");
        for (field, args) in optional_args {
            output += &format!(".{field}({args})");
        }
        output += "\n}";
        output
    }
}

pub fn replace_closure(cx: &rustc_lint::LateContext<'_>, mut closure: &BuilderClosure) -> String {
    if closure.builder_type == "CreateInteractionResponse" {
        replace_create_interaction_response(cx, &closure)
    } else if closure.builder_type == "CreateComponents" {
        replace_create_components(cx, &closure)
    } else {
        replace_generic(cx, &closure)
    }
}
