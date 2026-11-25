// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

use apollinaire_ascii::{
    Frame, MAX_INPUT_BYTES, MAX_WIDTH,
    diagram::{fenced_blocks, mermaid, mermaid_with_max_width, render_markdown},
};

fn assert_ascii(frame: &Frame) {
    assert!(
        frame.cells().iter().all(|b| (32..=126).contains(b)),
        "non-ASCII cell in frame:\n{}",
        frame.text()
    );
    assert!(
        frame
            .text()
            .bytes()
            .all(|b| b == b'\n' || (32..=126).contains(&b))
    );
}

#[test]
fn max_width_fits_all_five_diagram_types_and_refuses_impossible_bounds() {
    // Bounds validation.
    assert_eq!(
        mermaid_with_max_width("graph TD\nA-->B", 0).unwrap_err(),
        "--max-width must be between 1 and 512"
    );
    assert_eq!(
        mermaid_with_max_width("graph TD\nA-->B", MAX_WIDTH + 1).unwrap_err(),
        "--max-width must be between 1 and 512"
    );

    // Already within limit -> identical to default.
    let small = "graph LR\n  A --> B";
    assert_eq!(
        mermaid_with_max_width(small, 80).unwrap().text(),
        mermaid(small).unwrap().text()
    );

    // Wide LR flowchart -> automatically reflows to TD without wrapping.
    let wide_lr = "graph LR\n  A[Parse Input] --> B[Validate] --> C[Transform] --> D[Emit]";
    let reflowed = mermaid_with_max_width(wide_lr, 24).unwrap();
    assert_ascii(&reflowed);
    assert!(reflowed.width() <= 24, "width={}", reflowed.width());
    assert!(reflowed.text().contains("Parse Input"));

    // RL flowchart reflows to BT when narrowed, and wraps when even vertical exceeds limit.
    let wide_rl = "graph RL\n  A[VeryLongUnbrokenIdentifierPlusWords Here] -->|a long edge description| B[Target Node Label]";
    let wrapped_rl = mermaid_with_max_width(wide_rl, 18).unwrap();
    assert_ascii(&wrapped_rl);
    assert!(wrapped_rl.width() <= 18, "width={}", wrapped_rl.width());

    // State diagram with long label and self-loop wrapped to fit narrow width.
    let wide_state = "stateDiagram-v2\n  [*] --> Active\n  Active : waiting for incoming network packets\n  Active --> Active : exponential_backoff_retry\n  Active --> [*]";
    let fit_state = mermaid_with_max_width(wide_state, 20).unwrap();
    assert_ascii(&fit_state);
    assert!(fit_state.width() <= 20, "width={}", fit_state.width());

    // Very narrow width triggers truncate_ascii width <= 2 branch on self-loop.
    let tiny_loop = "stateDiagram-v2\n  A --> A : retry_now";
    let fit_tiny = mermaid_with_max_width(tiny_loop, 10).unwrap();
    assert_ascii(&fit_tiny);
    assert!(fit_tiny.width() <= 10, "width={}", fit_tiny.width());

    // Class diagram wrapped cleanly.
    let wide_class = "classDiagram\n  class ServiceManager {\n    +configuration_timeout_milliseconds: u64\n    +dispatch_background_request(payload) Result\n  }\n  ServiceManager --> WorkerPool : schedules work items";
    let fit_class = mermaid_with_max_width(wide_class, 24).unwrap();
    assert_ascii(&fit_class);
    assert!(fit_class.width() <= 24, "width={}", fit_class.width());

    // ER diagram wrapped cleanly.
    let wide_er = "erDiagram\n  ACCOUNT_HOLDER {\n    string registered_australian_business_number PK\n  }\n  ACCOUNT_HOLDER ||--o{ TAX_RETURN : lodges_annually";
    let fit_er = mermaid_with_max_width(wide_er, 24).unwrap();
    assert_ascii(&fit_er);
    assert!(fit_er.width() <= 24, "width={}", fit_er.width());

    // Sequence diagram: already fits, wrapped cleanly, and impossible width refused.
    let seq_small = "sequenceDiagram\n  A->>B: hi";
    assert_eq!(
        mermaid_with_max_width(seq_small, 40).unwrap().text(),
        mermaid(seq_small).unwrap().text()
    );
    let seq_wide = "sequenceDiagram\n  participant A as Authentication Service\n  participant B as Database Cluster\n  A->>B: verify user credentials and issue token\n  B-->>A\n  A->>A: record audit log entry\n  Note right of B: replicated across availability zones\n  alt retry until upstream responds cleanly\n    A->>B: ping\n  else fallback path\n    B->>A: cached\n  end\n";
    let fit_seq = mermaid_with_max_width(seq_wide, 36).unwrap();
    assert_ascii(&fit_seq);
    assert!(fit_seq.width() <= 36, "width={}", fit_seq.width());

    // Long text whose mid-point wrap exceeds MAX_HEIGHT (256 rows) still fits at a wider wrap.
    let tall_flow_text = format!("{} ", "abcde".repeat(180));
    let fit_tall_flow =
        mermaid_with_max_width(&format!("graph TD\n  A[\"{tall_flow_text}\"]"), 10).unwrap();
    assert_ascii(&fit_tall_flow);
    assert!(fit_tall_flow.width() <= 10);
    let tall_seq_text = "abcde".repeat(600);
    let fit_tall_seq =
        mermaid_with_max_width(&format!("sequenceDiagram\n  A->>B: {tall_seq_text}"), 22).unwrap();
    assert_ascii(&fit_tall_seq);
    assert!(fit_tall_seq.width() <= 22);

    // Impossible widths refused cleanly.
    assert!(
        mermaid_with_max_width("graph TD\n  A --> B\n  A --> C\n  A --> D", 6)
            .unwrap_err()
            .contains("cannot fit within 6 columns")
    );
    assert!(
        mermaid_with_max_width("sequenceDiagram\n  A->>B: hi\n  B->>C: yo", 6)
            .unwrap_err()
            .contains("cannot fit within 6 columns")
    );
}

#[test]
fn render_markdown_replaces_mermaid_blocks_and_preserves_surrounding_text() {
    // 0 mermaid blocks -> verbatim unchanged (including UTF-8, inline backticks/tildes and non-mermaid fences).
    let plain = "# Café Notes\n\n`inline` and ~~strike~~\n\n```rust\nfn main() {}\n```\n\n    ```mermaid\n    not a fence\n    ```\n\n```back`tick\nplain\n```\n";
    assert_eq!(render_markdown(plain, None).unwrap(), plain);

    // Non-mermaid fence containing literal ```mermaid must NOT be altered!
    let nested = "````markdown\n```mermaid\ngraph LR\n  A --> B\n```\n````\n";
    assert_eq!(render_markdown(nested, None).unwrap(), nested);

    // Unclosed non-mermaid fence at EOF passes through verbatim.
    let unclosed_rust = "```rust\nlet x = 1;\n";
    assert_eq!(render_markdown(unclosed_rust, None).unwrap(), unclosed_rust);

    // Multiple mermaid blocks (backticks, tildes, indented, CRLF, no trailing newline) + max_width.
    let doc = concat!(
        "# Architecture\r\n",
        "\r\n",
        "Intro text with café.\r\n",
        "\r\n",
        "```Mermaid\r\n",
        "stateDiagram-v2\r\n",
        "  [*] --> Ready\r\n",
        "```\r\n",
        "\r\n",
        "  ~~~mermaid\r\n",
        "  erDiagram\r\n",
        "    USER ||--o{ POST : writes\r\n",
        "  ~~~\r\n",
        "\r\n",
        "```mermaid\r\n",
        "graph LR\r\n",
        "  Alpha[First Stage] --> Beta[Second Stage] --> Gamma[Third Stage]\r\n",
        "```"
    );
    let rendered = render_markdown(doc, Some(24)).unwrap();
    assert!(rendered.starts_with("# Architecture\r\n\r\nIntro text with café.\r\n\r\n```text\r\n"));
    assert!(rendered.contains("| (*) |"));
    assert!(rendered.contains("  ~~~text\r\n"));
    assert!(rendered.contains("||--o{ writes"));
    assert!(rendered.contains("First Stage"));
    assert!(!rendered.contains("```Mermaid"));
    assert!(!rendered.contains("~~~mermaid"));
    assert!(!rendered.ends_with('\n'));
    // Idempotent: running again changes nothing.
    assert_eq!(render_markdown(&rendered, Some(24)).unwrap(), rendered);

    // Errors in markdown mode.
    assert_eq!(
        render_markdown("# doc\n```mermaid\ngraph TD\n  A --> B\n", None).unwrap_err(),
        "Line 2: the mermaid code block is not closed"
    );
    assert!(
        render_markdown("```mermaid\npie\n```\n", None)
            .unwrap_err()
            .starts_with("Line 1: ")
    );
    assert_eq!(
        render_markdown("# empty", Some(0)).unwrap_err(),
        "--max-width must be between 1 and 512"
    );
    let huge = " ".repeat(MAX_INPUT_BYTES + 1);
    assert!(render_markdown(&huge, None).unwrap_err().contains("16 MiB"));
}

#[test]
fn render_markdown_widens_fences_past_backtick_and_tilde_runs_in_diagram() {
    // 3-backtick edge label rendered on its own indented line must widen ``` to ````.
    let three_ticks = "```mermaid\ngraph TD\n  A -->|```| B\n```\n";
    let rendered_three = render_markdown(three_ticks, None).unwrap();
    assert!(rendered_three.starts_with("````text\n"), "{rendered_three}");
    assert!(rendered_three.ends_with("\n````\n"), "{rendered_three}");
    assert_eq!(fenced_blocks(&rendered_three).len(), 1);
    assert_eq!(
        render_markdown(&rendered_three, None).unwrap(),
        rendered_three
    );

    // 4+ backticks (via Mermaid HTML entities or wider input fence) widen to max_run + 1.
    let four_ticks = "```mermaid\ngraph TD\n  A -->|#96;#96;#96;#96;| B\n```\n";
    let rendered_four = render_markdown(four_ticks, None).unwrap();
    assert!(rendered_four.starts_with("`````text\n"), "{rendered_four}");
    assert!(rendered_four.ends_with("\n`````\n"), "{rendered_four}");
    assert_eq!(fenced_blocks(&rendered_four).len(), 1);

    let six_ticks_already_wide = "``````mermaid\ngraph TD\n  A -->|#96;#96;#96;| B\n``````\n";
    let rendered_wide = render_markdown(six_ticks_already_wide, None).unwrap();
    assert!(rendered_wide.starts_with("``````text\n"), "{rendered_wide}");
    assert!(rendered_wide.ends_with("\n``````\n"), "{rendered_wide}");
    assert_eq!(fenced_blocks(&rendered_wide).len(), 1);

    // 3-tilde and 4-tilde runs inside tilde-fenced diagrams widen tilde fences symmetrically.
    let three_tildes = "  ~~~mermaid\n  graph TD\n    A -->|~~~| B\n  ~~~\n";
    let rendered_tildes = render_markdown(three_tildes, None).unwrap();
    assert!(
        rendered_tildes.starts_with("  ~~~~text\n"),
        "{rendered_tildes}"
    );
    assert!(rendered_tildes.ends_with("\n  ~~~~\n"), "{rendered_tildes}");
    assert_eq!(fenced_blocks(&rendered_tildes).len(), 1);
    assert_eq!(
        render_markdown(&rendered_tildes, None).unwrap(),
        rendered_tildes
    );

    let four_tildes = "~~~mermaid\ngraph TD\n  A -->|~~~~| B\n~~~\n";
    let rendered_four_tildes = render_markdown(four_tildes, None).unwrap();
    assert!(
        rendered_four_tildes.starts_with("~~~~~text\n"),
        "{rendered_four_tildes}"
    );
    assert!(
        rendered_four_tildes.ends_with("\n~~~~~\n"),
        "{rendered_four_tildes}"
    );
    assert_eq!(fenced_blocks(&rendered_four_tildes).len(), 1);

    // Tildes inside a backtick fence cannot close a backtick fence in CommonMark.
    let tildes_in_ticks = "```mermaid\ngraph TD\n  A -->|~~~| B\n```\n";
    let rendered_mixed = render_markdown(tildes_in_ticks, None).unwrap();
    assert!(rendered_mixed.starts_with("```text\n"), "{rendered_mixed}");
    assert!(rendered_mixed.ends_with("\n```\n"), "{rendered_mixed}");
    assert_eq!(fenced_blocks(&rendered_mixed).len(), 1);
}
