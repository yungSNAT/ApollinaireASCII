// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

use apollinaire_ascii::{
    Frame, MAX_INPUT_BYTES,
    diagram::{class_diagram, er_diagram, mermaid, state_diagram},
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
fn state_diagram_draws_start_end_labels_descriptions_and_self_loops() {
    let source = "
%% State machine for order lifecycle
stateDiagram-v2
  state \"Waiting for review\" as Idle
  Idle : queued in inbox
  Idle : awaiting approval
  Idle :
  [*] --> Idle
  Idle --> Active : start
  Active --> Active : retry
  Active --> Active : tick
  Active --> Active
  Wait --> Wait
  Wait --> Wait : wake
  Quiet --> Quiet
  Active --> Done : finish
  Done --> [*]
  state Archived
  classDef dim fill:#eee
  class Idle dim
  style Done stroke:#333
";
    let frame = mermaid(source).unwrap();
    assert_ascii(&frame);
    let text = frame.text();
    assert!(text.contains("| (*) |"), "{text}");
    assert!(text.contains("Waiting for review"), "{text}");
    assert!(text.contains("queued in inbox"), "{text}");
    assert!(text.contains("awaiting approval"), "{text}");
    assert!(text.contains("retry, tick"), "{text}");
    assert!(text.contains("| Done |"), "{text}");
    assert!(text.contains("| Archived |"), "{text}");
    // Start and end [*] must be separate nodes so (*) appears twice.
    assert_eq!(text.matches("| (*) |").count(), 2, "{text}");

    // Horizontal flow with self-loop (both labelled and unlabelled) and legacy `stateDiagram` header.
    let horiz = state_diagram(
        "stateDiagram; direction LR\n  [*] --> Ready\n  Ready --> Ready : poll\n  Idle --> Idle\n  Ready --> [*]\n  Standalone\n",
    )
    .unwrap();
    assert_ascii(&horiz);
    assert!(horiz.text().contains("poll"), "{}", horiz.text());
    assert!(horiz.text().contains("Standalone"), "{}", horiz.text());

    // Entity decoding and BT / RL directions.
    let bt = state_diagram("stateDiagram-v2\n  direction BT\n  A --> B : a#59;b\n").unwrap();
    assert_ascii(&bt);
    assert!(bt.text().contains("a;b"), "{}", bt.text());

    let rl = state_diagram("stateDiagram-v2\n  direction RL\n  A --> B\n").unwrap();
    assert_ascii(&rl);
    assert!(rl.text().contains("<"), "{}", rl.text());
}

#[test]
fn state_diagram_refuses_invalid_syntax_and_limits() {
    for (source, expected) in [
        (
            "",
            "A state diagram starts with stateDiagram-v2 or stateDiagram",
        ),
        (
            "graph TD\nA-->B",
            "Line 1: a state diagram starts with stateDiagram-v2 or stateDiagram",
        ),
        (
            "stateDiagram-v2",
            "A state diagram needs at least one state",
        ),
        (
            "stateDiagram-v2\n  direction",
            "Line 2: direction needs one of TD, TB, BT, LR or RL",
        ),
        (
            "stateDiagram-v2\n  direction LR extra",
            "Line 2: unexpected text after direction LR",
        ),
        (
            "stateDiagram-v2\n  direction DIAG",
            "Line 2: the direction DIAG is not one of TD, TB, BT, LR or RL",
        ),
        (
            "stateDiagram-v2\n  A-B --> C",
            "Line 2: a state is named with ASCII letters, digits and _, or [*], not 'A-B'",
        ),
        (
            "stateDiagram-v2\n  A-B : bad",
            "Line 2: a state is named with ASCII letters, digits and _, not 'A-B'",
        ),
        (
            "stateDiagram-v2\n  state Composite {",
            "Line 2: composite states are not supported",
        ),
        (
            "stateDiagram-v2\n  state \"unclosed",
            "Line 2: state description quote is not closed",
        ),
        (
            "stateDiagram-v2\n  state \"ok\" Idle",
            "Line 2: expected 'as <state>' after state description",
        ),
        (
            "stateDiagram-v2\n  state \"ok\" as Bad-Id",
            "Line 2: a state is named with ASCII letters, digits and _, not 'Bad-Id'",
        ),
        (
            "stateDiagram-v2\n  state Bad-Id",
            "Line 2: a state is named with ASCII letters, digits and _, not 'Bad-Id'",
        ),
        (
            "stateDiagram-v2\n  note right of A : hi",
            "Line 2: 'note right of A : hi' is not a state diagram statement this reads",
        ),
    ] {
        assert_eq!(state_diagram(source).unwrap_err(), expected, "{source:?}");
    }

    let huge = " ".repeat(MAX_INPUT_BYTES + 1);
    assert!(state_diagram(&huge).unwrap_err().contains("16 MiB"));

    let many_states: String = (0..1025).map(|i| format!("  S{i}\n")).collect();
    assert!(
        state_diagram(&format!("stateDiagram-v2\n{many_states}"))
            .unwrap_err()
            .contains("1024 states")
    );

    let many_links: String = (0..4097).map(|_| "  A --> B\n").collect();
    assert!(
        state_diagram(&format!("stateDiagram-v2\n{many_links}"))
            .unwrap_err()
            .contains("4096 transitions")
    );

    // Aggregate compartment limit across multiple states (S1).
    let many_descs: String = (0..4097)
        .map(|i| format!("  S{} : d{i}\n", i % 4))
        .collect();
    assert!(
        state_diagram(&format!("stateDiagram-v2\n{many_descs}"))
            .unwrap_err()
            .contains("4096 descriptions")
    );
}

#[test]
fn class_diagram_draws_compartments_stereotypes_and_all_relationships() {
    let source = "
%% Class hierarchy (The Mentalist nod in fixture names)
classDiagram
  class Consultant[\"CBI Consultant\"] {
    <<abstract>>
    +String name
    -int cases_closed

    +observe() bool
    +deduce(clues) String }
  <<interface>> Investigator
  class Investigator { +investigate(); +report() }
  class Badge { +int id }
  class Empty {}
  Foo--Bar : +x
  Badge \"1\" --> Empty
  class Jane { +String tea
  }
  class Lisbon
  Lisbon : +String rank
  Lisbon : +lead()
  Lisbon :
  Consultant <|-- Jane : extends
  Investigator <|.. Lisbon
  Jane --|> Consultant
  Lisbon ..|> Investigator
  Team *-- Jane : \"1\"
  Team \"1\" *-- \"*\" Lisbon : members
  Squad --* Unit
  Office o-- Desk
  Desk --o Room
  Jane --> Lisbon : advises
  Lisbon <-- Jane
  Jane <--> Lisbon : partners
  Jane ..> TeaCup : drinks
  TeaCup <.. Jane
  Jane <..> TeaCup
  Unit-One -- Unit-Two
  Unit-One .. Unit-Two
  Jane --> Jane : reflects
  classDef highlight fill:#ff0
  style Jane stroke:#333
  cssClass Jane highlight
";
    let frame = mermaid(source).unwrap();
    assert_ascii(&frame);
    let text = frame.text();
    assert!(text.contains("<<abstract>>"), "{text}");
    assert!(text.contains("CBI Consultant"), "{text}");
    assert!(text.contains("+String name"), "{text}");
    assert!(text.contains("+observe() bool"), "{text}");
    assert!(text.contains("+deduce(clues) String"), "{text}");
    assert!(text.contains("<<interface>>"), "{text}");
    assert!(text.contains("+investigate()"), "{text}");
    assert!(text.contains("<|-- extends"), "{text}");
    assert!(text.contains("1..* members"), "{text}");
    assert!(text.contains("*"), "{text}");
    assert!(text.contains("o"), "{text}");
    assert!(text.contains(":"), "{text}");
    assert!(text.contains("reflects"), "{text}");

    // Horizontal direction and quote-safe relation detection.
    let lr = class_diagram(
        "classDiagram\n  direction LR\n  A \"1..2\" --> \"0..1\" B : uses\n  A --> C : \"a#59;b\"\n",
    )
    .unwrap();
    assert_ascii(&lr);
    assert!(lr.text().contains("1..2..0..1 uses"), "{}", lr.text());
    assert!(lr.text().contains("a;b"), "{}", lr.text());
}

#[test]
fn class_diagram_refuses_invalid_syntax_and_limits() {
    for (source, expected) in [
        ("", "A class diagram starts with classDiagram"),
        (
            "graph TD\nA-->B",
            "Line 1: a class diagram starts with classDiagram",
        ),
        ("classDiagram", "A class diagram needs at least one class"),
        (
            "classDiagram\n  class Foo {\n    +int x",
            "The class block opened on line 2 is not closed",
        ),
        ("classDiagram\n  }", "Line 2: } closes no class block"),
        (
            "classDiagram\n  direction",
            "Line 2: direction needs one of TD, TB, BT, LR or RL",
        ),
        (
            "classDiagram\n  direction LR extra",
            "Line 2: unexpected text after direction LR",
        ),
        (
            "classDiagram\n  direction BAD",
            "Line 2: the direction BAD is not one of TD, TB, BT, LR or RL",
        ),
        (
            "classDiagram\n  class Bad!Name",
            "Line 2: a class is named with ASCII letters, digits, _, - or ~, not 'Bad!Name'",
        ),
        (
            "classDiagram\n  class Foo[\"unclosed",
            "Line 2: class label bracket is not closed",
        ),
        (
            "classDiagram\n  namespace App {",
            "Line 2: 'namespace App {' is not a class diagram statement this reads",
        ),
    ] {
        assert_eq!(class_diagram(source).unwrap_err(), expected, "{source:?}");
    }

    let huge = " ".repeat(MAX_INPUT_BYTES + 1);
    assert!(class_diagram(&huge).unwrap_err().contains("16 MiB"));

    let many_classes: String = (0..1025).map(|i| format!("  class C{i}\n")).collect();
    assert!(
        class_diagram(&format!("classDiagram\n{many_classes}"))
            .unwrap_err()
            .contains("1024 classes")
    );

    let many_links: String = (0..4097).map(|_| "  A --> B\n").collect();
    assert!(
        class_diagram(&format!("classDiagram\n{many_links}"))
            .unwrap_err()
            .contains("4096 relationships")
    );

    // Aggregate compartment limit across members and inline stereotypes (S1).
    let many_members: String = (0..4097)
        .map(|i| format!("  C{} : +m{i}\n", i % 4))
        .collect();
    assert!(
        class_diagram(&format!("classDiagram\n{many_members}"))
            .unwrap_err()
            .contains("4096 members")
    );
    let many_stereos: String = (0..4097)
        .map(|i| format!("  <<tag{i}>> C{}\n", i % 4))
        .collect();
    assert!(
        class_diagram(&format!("classDiagram\n{many_stereos}"))
            .unwrap_err()
            .contains("4096 members")
    );
}

#[test]
fn er_diagram_draws_entities_attributes_and_cardinalities() {
    let source = "
%% Commerce schema
erDiagram
  CUSTOMER {
    int id PK
    string name \"full name\"

    string email UK }
  ORDER { int id PK; date created_at }
  NOTE { string body }
  EMPTY {}
  LINE-ITEM {
    int qty
  }
  TAG { string label
  }
  AUDIT_LOG
  CUSTOMER ||--o{ ORDER : places
  ORDER ||--|{ LINE-ITEM : \"contains items\"
  CUSTOMER |o..o| PROFILE : optional
  PROFILE }|..}| GROUP : belongs
  GROUP }o--o{ TAG
  EMPLOYEE ||--o{ EMPLOYEE : manages
  classDef muted fill:#eee
  class TAG muted
  style ORDER stroke:#111
";
    let frame = mermaid(source).unwrap();
    assert_ascii(&frame);
    let text = frame.text();
    assert!(text.contains("CUSTOMER"), "{text}");
    assert!(text.contains("int id PK"), "{text}");
    assert!(text.contains("string email UK"), "{text}");
    assert!(text.contains("date created_at"), "{text}");
    assert!(text.contains("||--o{ places"), "{text}");
    assert!(text.contains("||--|{ contains items"), "{text}");
    assert!(text.contains("|o..o| optional"), "{text}");
    assert!(text.contains("}o--o{"), "{text}");
    assert!(text.contains("||--o{ manages"), "{text}");
    assert!(text.contains("AUDIT_LOG"), "{text}");

    let lr = er_diagram("erDiagram\n  direction LR\n  A ||--|| B : links\n").unwrap();
    assert_ascii(&lr);
    assert!(lr.text().contains("||--|| links"), "{}", lr.text());
}

#[test]
fn er_diagram_refuses_invalid_syntax_and_limits() {
    for (source, expected) in [
        ("", "An ER diagram starts with erDiagram"),
        (
            "graph TD\nA-->B",
            "Line 1: an ER diagram starts with erDiagram",
        ),
        ("erDiagram", "An ER diagram needs at least one entity"),
        (
            "erDiagram\n  USER {\n    int id",
            "The entity block opened on line 2 is not closed",
        ),
        ("erDiagram\n  }", "Line 2: } closes no entity block"),
        (
            "erDiagram\n  direction",
            "Line 2: direction needs one of TD, TB, BT, LR or RL",
        ),
        (
            "erDiagram\n  direction TD extra",
            "Line 2: unexpected text after direction TD",
        ),
        (
            "erDiagram\n  direction BAD",
            "Line 2: the direction BAD is not one of TD, TB, BT, LR or RL",
        ),
        (
            "erDiagram\n  BAD!NAME {\n  }",
            "Line 2: an entity is named with ASCII letters, digits, _ or -, not 'BAD!NAME'",
        ),
        (
            "erDiagram\n  A ||==o{ B : bad",
            "Line 2: 'A ||==o{ B : bad' is not an ER diagram statement this reads",
        ),
        (
            "erDiagram\n  A ~~--~~ B : bad",
            "Line 2: 'A ~~--~~ B : bad' is not an ER diagram statement this reads",
        ),
        (
            "erDiagram\n  A ||-o{ B : short",
            "Line 2: 'A ||-o{ B : short' is not an ER diagram statement this reads",
        ),
        (
            "erDiagram\n  not valid syntax here",
            "Line 2: 'not valid syntax here' is not an ER diagram statement this reads",
        ),
    ] {
        assert_eq!(er_diagram(source).unwrap_err(), expected, "{source:?}");
    }

    let huge = " ".repeat(MAX_INPUT_BYTES + 1);
    assert!(er_diagram(&huge).unwrap_err().contains("16 MiB"));

    let many_entities: String = (0..1025).map(|i| format!("  E{i}\n")).collect();
    assert!(
        er_diagram(&format!("erDiagram\n{many_entities}"))
            .unwrap_err()
            .contains("1024 entities")
    );

    let many_links: String = (0..4097).map(|_| "  A ||--|| B : rel\n").collect();
    assert!(
        er_diagram(&format!("erDiagram\n{many_links}"))
            .unwrap_err()
            .contains("4096 relationships")
    );

    // Aggregate compartment limit across multiple entities (S1).
    let many_attrs: String = (0..4)
        .map(|e| {
            let attrs: String = (0..1025).map(|i| format!("    int f{i}\n")).collect();
            format!("  E{e} {{\n{attrs}  }}\n")
        })
        .collect();
    assert!(
        er_diagram(&format!("erDiagram\n{many_attrs}"))
            .unwrap_err()
            .contains("4096 attributes")
    );
}
