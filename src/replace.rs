//! Provides functions to transform our structures into serenity 0.12 compatible builders

use rustc_lint::LintContext as _;

use crate::structures::*;

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
) -> Option<String> {
    source.span_to_snippet(rustc_span::hygiene::walk_chain(span, syntax_ctxt)).ok()
}

fn field_arg_string(
    cx: &rustc_lint::LateContext<'_>,
    syntax_ctxt: rustc_span::SyntaxContext,
    args: Vec<BuilderCallArg>,
) -> String {
    args.into_iter()
        .map(|arg| match arg {
            BuilderCallArg::Literal(expr) => {
                span_to_source(cx.sess().source_map(), syntax_ctxt, expr.span)
                    .unwrap_or("todo!()".to_owned())
            }
            BuilderCallArg::NestedClosure(closure) => replace_closure(cx, closure),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn replace_builder_call_chain_stmt(
    cx: &rustc_lint::LateContext<'_>,
    syntax_ctxt: rustc_span::SyntaxContext,
    call_chain: BuilderCallChain,
) -> String {
    let builder_ident = call_chain.receiver;
    let mut line = format!("{builder_ident} = {builder_ident}");
    for BuilderCall { field, args } in call_chain.calls {
        let arg_string = field_arg_string(cx, syntax_ctxt, args);
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
    mut closure: BuilderClosure,
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
    let mut fields = Vec::new();
    if let Some(call) = closure
        .call_chain
        .calls
        .into_iter()
        .find(|call| call.field.as_str() == "interaction_response_data")
    {
        if let Some(BuilderCallArg::NestedClosure(closure)) = call.args.into_iter().next() {
            assert!(closure.stmts.is_empty()); // Ignoring stmts for now
            fields = closure.call_chain.calls;
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

fn replace_generic(cx: &rustc_lint::LateContext<'_>, mut closure: BuilderClosure) -> String {
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
    for call in closure.call_chain.calls {
        if required_fields.contains(&call.field.as_str()) {
            required_args.push(field_arg_string(cx, syntax_ctxt, call.args));
        } else {
            optional_args.push((call.field, field_arg_string(cx, syntax_ctxt, call.args)));
        }
    }

    let binding = closure.binding;
    let ty = closure.builder_type;
    let required_args = required_args.join(", ");
    if closure.stmts.is_empty() {
        let mut output = format!("{ty}::new({required_args})");
        for (field, args) in optional_args {
            output += &format!("\n.{field}({args})");
        }
        output
    } else {
        let mut stmts_string = String::new();
        for stmt in closure.stmts {
            let line = match stmt {
                PreBuilderCallStatement::Verbatim(stmt_span) => {
                    span_to_source(cx.sess().source_map(), syntax_ctxt, stmt_span)
                        .unwrap_or_default()
                }
                PreBuilderCallStatement::BuilderCallChain(call_chain) => {
                    replace_builder_call_chain_stmt(cx, syntax_ctxt, call_chain)
                }
            };
            stmts_string += &line;
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

pub fn replace_closure(cx: &rustc_lint::LateContext<'_>, mut closure: BuilderClosure) -> String {
    if closure.builder_type == "CreateInteractionResponse" {
        replace_create_interaction_response(cx, closure)
    } else {
        replace_generic(cx, closure)
    }
}
