// Copyright 2025 Bloxide, all rights reserved
//! Context view (#124) and supervision tree view (#123).
use crate::model::*;
use dioxus::prelude::*;

// ── Context view (#124): context struct, actions, and message definitions ──

#[component]
pub(crate) fn ContextView(spec: BloxSpec) -> Element {
    rsx! {
        div {
            style: "display: flex; gap: 20px; align-items: flex-start; max-width: 1100px;",

            // Context struct
            div {
                style: "min-width: 320px; background: #f9fafb; border-radius: 8px; padding: 16px;",
                h3 { style: "margin: 0 0 12px 0; color: #1f2937; font-size: 15px;", "Context" }
                if let Some(ctx) = &spec.context {
                    div {
                        style: "margin-bottom: 12px; font-family: monospace; font-weight: 600; color: #1e40af;",
                        "pub struct {ctx.struct_name}"
                    }
                    for field in &ctx.fields {
                        div {
                            style: "display: flex; justify-content: space-between; gap: 12px; padding: 6px 10px; background: white; border-radius: 4px; margin-bottom: 4px; font-size: 13px;",
                            span { style: "font-family: monospace; color: #1f2937;", "pub {field.name}" }
                            span { style: "font-family: monospace; color: #6b7280;", "{field.ty}" }
                        }
                    }
                    if !ctx.uses.is_empty() {
                        div { style: "margin-top: 12px; font-size: 12px; color: #6b7280; margin-bottom: 6px;", "From [[context.uses]]" }
                        for u in &ctx.uses {
                            div {
                                style: "display: flex; justify-content: space-between; gap: 12px; padding: 6px 10px; background: #eff6ff; border-radius: 4px; margin-bottom: 4px; font-size: 13px;",
                                span { style: "font-family: monospace; color: #1e40af;", "pub {u.name}" }
                                span { style: "font-family: monospace; color: #6b7280;", "{u.ty}" }
                            }
                        }
                    }
                } else {
                    div { style: "color: #9ca3af; font-size: 13px;", "No context definition (message crate)" }
                }
            }

            // Actions
            div {
                style: "min-width: 380px; background: #f9fafb; border-radius: 8px; padding: 16px;",
                h3 { style: "margin: 0 0 12px 0; color: #1f2937; font-size: 15px;", "Actions" }
                if spec.actions.is_empty() {
                    div { style: "color: #9ca3af; font-size: 13px;", "No action declarations" }
                }
                for action in &spec.actions {
                    div {
                        style: "margin-bottom: 8px; padding: 8px 10px; background: white; border-radius: 4px;",
                        div { style: "font-size: 11px; color: #6b7280;", "{action.crate_name}" }
                        div { style: "font-family: monospace; font-size: 12px; color: #374151; white-space: pre-wrap; word-break: break-all;", "{action.signature}" }
                    }
                }
            }

            // Messages
            div {
                style: "min-width: 280px; background: #f9fafb; border-radius: 8px; padding: 16px;",
                h3 { style: "margin: 0 0 12px 0; color: #1f2937; font-size: 15px;", "Messages" }
                if spec.messages.is_empty() {
                    div { style: "color: #9ca3af; font-size: 13px;", "No message definitions here — see the *-messages spec tabs" }
                }
                for msg in &spec.messages {
                    div {
                        style: "margin-bottom: 10px; padding: 10px; background: white; border-radius: 4px;",
                        div { style: "font-size: 11px; color: #6b7280;", "{msg.crate_name}" }
                        div { style: "font-family: monospace; font-weight: 600; color: #1f2937; margin-bottom: 6px;", "pub enum {msg.enum_name}" }
                        for v in &msg.variants {
                            div {
                                style: "font-family: monospace; font-size: 12px; color: #374151; padding: 2px 0;",
                                "{v.name}"
                                if !v.fields.is_empty() {
                                    span { style: "color: #6b7280;", "({v.fields.join(\", \")})" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── Supervision tree view (#123): supervisor hierarchy with policies ───────

#[component]
pub(crate) fn SupervisionTreeView(spec: BloxSpec) -> Element {
    let Some(wiring) = &spec.wiring else {
        return rsx! {
            div {
                style: "padding: 40px; text-align: center; color: #6b7280; font-size: 14px;",
                "This spec has no supervision data. "
                "Select a system spec tab (an app like tokio-pool-demo) to see its supervision tree."
            }
        };
    };

    if wiring.supervisors.is_empty() {
        return rsx! {
            div {
                style: "padding: 40px; text-align: center; color: #6b7280; font-size: 14px;",
                "No [[supervision]] sections in this system."
            }
        };
    }

    rsx! {
        div {
            style: "display: flex; gap: 24px; align-items: flex-start; padding: 12px;",
            for sup in &wiring.supervisors {
                div {
                    style: "background: #f9fafb; border-radius: 8px; padding: 16px; min-width: 280px;",
                    // Supervisor node
                    div {
                        style: "padding: 12px 16px; background: #6366f1; color: white; border-radius: 6px; text-align: center; margin-bottom: 4px;",
                        div { style: "font-weight: 700; font-size: 14px;", "{sup.name}" }
                        div { style: "font-size: 11px; opacity: 0.85;", "strategy: {sup.strategy}" }
                    }
                    // Failure escalation arrow
                    div {
                        style: "text-align: center; color: #9ca3af; font-size: 12px; margin: 4px 0;",
                        "▲ child failure events (Started / Stopped / Done / Failed / Aborted / Killed)"
                    }
                    // Children with policy badges
                    for child in &sup.children {
                        div {
                            style: "display: flex; justify-content: space-between; align-items: center; gap: 10px; padding: 10px 12px; background: white; border-radius: 4px; margin-top: 8px; border: 1px solid #e5e7eb;",
                            span { style: "font-family: monospace; font-size: 13px; color: #1f2937;", "{child.actor}" }
                            span {
                                style: "display: flex; gap: 6px;",
                                if let Some(max) = child.restart_max {
                                    span {
                                        style: "padding: 2px 8px; background: #dcfce7; color: #166534; border-radius: 9999px; font-size: 11px; font-weight: 600;",
                                        "restart: max {max}"
                                    }
                                }
                                if child.stop == Some(true) {
                                    span {
                                        style: "padding: 2px 8px; background: #fee2e2; color: #991b1b; border-radius: 9999px; font-size: 11px; font-weight: 600;",
                                        "stop"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
