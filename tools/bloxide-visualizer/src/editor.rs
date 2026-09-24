// Copyright 2025 Bloxide, all rights reserved
//! Editor panel (#96): add states / transitions from the UI.
use crate::model::*;
use crate::server::{viz_add_state, viz_add_transition};
use dioxus::prelude::*;

fn parse_csv(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[component]
pub(crate) fn EditorPanel(spec: BloxSpec, specs: Signal<Vec<BloxSpec>>) -> Element {
    let path = format!("{}/blox.toml", spec.crate_path);
    let path_add_state = path.clone();
    let path_add_transition = path.clone();
    let mut show_state_form = use_signal(|| false);
    let mut show_trans_form = use_signal(|| false);
    let mut edit_status = use_signal(|| None::<String>);

    // State form fields
    let mut state_name = use_signal(String::new);
    let mut state_parent = use_signal(String::new);
    let mut state_composite = use_signal(|| false);
    let mut state_error = use_signal(|| false);

    // Transition form fields
    let mut t_state = use_signal(String::new);
    let mut t_event = use_signal(String::new);
    let mut t_target = use_signal(String::new);
    let mut t_actions = use_signal(String::new);
    let mut t_guards = use_signal(String::new);

    let input_style = "padding: 6px 10px; border: 1px solid #d1d5db; border-radius: 4px; font-size: 13px; font-family: monospace;";
    let btn_style = "padding: 6px 14px; background: #6366f1; color: white; border: none; border-radius: 4px; cursor: pointer; font-size: 13px;";

    rsx! {
        div {
            style: "margin-bottom: 12px; display: flex; gap: 8px; align-items: center; flex-wrap: wrap;",
            button {
                style: "{btn_style}",
                onclick: move |_| show_state_form.set(!show_state_form()),
                "+ State"
            }
            button {
                style: "{btn_style}",
                onclick: move |_| show_trans_form.set(!show_trans_form()),
                "+ Transition"
            }
            if let Some(status) = edit_status() {
                span { style: "font-size: 12px; color: #6b7280;", "{status}" }
            }
        }
        if show_state_form() {
            div {
                id: "state-form",
                style: "margin-bottom: 12px; padding: 12px; background: #f9fafb; border-radius: 6px; display: flex; gap: 8px; align-items: center; flex-wrap: wrap;",
                input {
                    style: "{input_style}",
                    placeholder: "StateName",
                    name: "state-name",
                    value: "{state_name}",
                    oninput: move |e| state_name.set(e.value()),
                }
                input {
                    style: "{input_style}",
                    placeholder: "parent (optional)",
                    name: "state-parent",
                    value: "{state_parent}",
                    oninput: move |e| state_parent.set(e.value()),
                }
                label {
                    style: "font-size: 13px; color: #374151;",
                    input {
                        r#type: "checkbox",
                        name: "state-composite",
                        checked: "{state_composite}",
                        onchange: move |e| state_composite.set(e.checked()),
                    }
                    " composite"
                }
                label {
                    style: "font-size: 13px; color: #374151;",
                    input {
                        r#type: "checkbox",
                        name: "state-error",
                        checked: "{state_error}",
                        onchange: move |e| state_error.set(e.checked()),
                    }
                    " error"
                }
                button {
                    style: "{btn_style}",
                    onclick: move |_| {
                        let name = state_name();
                        let parent = {
                            let p = state_parent();
                            if p.is_empty() { None } else { Some(p) }
                        };
                        let (composite, error) = (state_composite(), state_error());
                        let path_add_state = path_add_state.clone();
                        async move {
                            if name.is_empty() {
                                edit_status.set(Some("state name required".to_string()));
                                return;
                            }
                            match viz_add_state(path_add_state, name.clone(), parent, composite, error).await {
                                Ok(new_specs) => {
                                    specs.set(new_specs);
                                    edit_status.set(Some(format!("added state {}", name)));
                                    state_name.set(String::new());
                                    state_parent.set(String::new());
                                }
                                Err(e) => edit_status.set(Some(format!("error: {}", e))),
                            }
                        }
                    },
                    "Add State"
                }
            }
        }
        if show_trans_form() {
            div {
                id: "transition-form",
                style: "margin-bottom: 12px; padding: 12px; background: #f9fafb; border-radius: 6px; display: flex; gap: 8px; align-items: center; flex-wrap: wrap;",
                input {
                    style: "{input_style}",
                    placeholder: "from state",
                    name: "trans-state",
                    value: "{t_state}",
                    oninput: move |e| t_state.set(e.value()),
                }
                input {
                    style: "{input_style} min-width: 280px;",
                    placeholder: "event pattern (e.g. MyMsg::Tick(_))",
                    name: "trans-event",
                    value: "{t_event}",
                    oninput: move |e| t_event.set(e.value()),
                }
                input {
                    style: "{input_style}",
                    placeholder: "target (state | stay | done | stop | reset | fail)",
                    name: "trans-target",
                    value: "{t_target}",
                    oninput: move |e| t_target.set(e.value()),
                }
                input {
                    style: "{input_style}",
                    placeholder: "actions (csv, optional)",
                    name: "trans-actions",
                    value: "{t_actions}",
                    oninput: move |e| t_actions.set(e.value()),
                }
                input {
                    style: "{input_style} min-width: 220px;",
                    placeholder: "guards (csv cond:target, optional)",
                    name: "trans-guards",
                    value: "{t_guards}",
                    oninput: move |e| t_guards.set(e.value()),
                }
                button {
                    style: "{btn_style}",
                    onclick: move |_| {
                        let (state, event, target) = (t_state(), t_event(), t_target());
                        let actions = parse_csv(&t_actions());
                        let guards = parse_csv(&t_guards());
                        let path_add_transition = path_add_transition.clone();
                        async move {
                            if state.is_empty() || event.is_empty() || target.is_empty() {
                                edit_status.set(Some("state, event, and target are required".to_string()));
                                return;
                            }
                            match viz_add_transition(path_add_transition, state.clone(), event.clone(), target, actions, guards).await {
                                Ok(new_specs) => {
                                    specs.set(new_specs);
                                    edit_status.set(Some(format!("added transition {} + {}", state, event)));
                                }
                                Err(e) => edit_status.set(Some(format!("error: {}", e))),
                            }
                        }
                    },
                    "Add Transition"
                }
            }
        }
    }
}
