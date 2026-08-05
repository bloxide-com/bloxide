// Copyright 2025 Bloxide, all rights reserved
mod app;
mod context_view;
mod data;
mod diagram;
mod editor;
mod model;
mod server;
mod system_view;

fn main() {
    dioxus::launch(app::App);
}
