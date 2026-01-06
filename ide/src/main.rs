use dioxus::prelude::*;

pub fn app() -> Element {
    let mut count = use_signal(|| 0);

    // Text + line count
    let mut text = use_signal(String::new);
    let mut line_count = use_signal(|| 1);

    rsx! {
        // Styles (note escaped braces)
        style { r#"
            .editor {{
                display: flex;
                font-family: monospace;
            }}
            .lines {{
                padding: 6px;
                text-align: right;
                background: #f0f0f0;
                color: #666;
                user-select: none;
            }}
            textarea {{
                resize: none;
                font-family: monospace;
            }}
        "# }

        h1 { "High-Five counter: {count}" }

        button {
            onclick: move |_| count += 1,
            "Up High!"
        }

        button {
            onclick: move |_| count -= 1,
            "Down Low!"
        }

        div { class: "editor",
            // Line numbers
            div { class: "lines",
                for line_no in 1..=line_count() {
                    div { "{line_no}" }
                }
            }

            // Text editor
            textarea {
                cols: "30",
                rows: "10",
                value: text,

                oninput: move |e| {
                    let value = e.value();
                    text.set(value.clone());

                    // Count lines based on newlines
                    let lines = value.matches('\n').count() + 1;
                    line_count.set(lines);
                },

                placeholder: "Type something...",
            }
        }
    }
}

fn main() {
    dioxus::launch(app);
}
