// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Mermaid sequence diagrams: participants, messages, notes, blocks and the limits.

use apollinaire_ascii::diagram::{mermaid, sequence};

fn drawn(source: &str) -> String {
    sequence(source).unwrap().text()
}

fn refused(source: &str) -> String {
    sequence(source).unwrap_err()
}

// The expected drawings follow the layout rules: a participant's box is its text plus 4 wide,
// centred on its lifeline and repeated at the bottom; boxes stand at least 2 columns apart and
// further where a message's text, a note or a loop needs the room; message text sits centred
// on the row above its arrow, and an arrowhead stops one cell short of the lifeline.
#[test]
fn messages_run_between_lifelines_with_their_text_above() {
    assert_eq!(
        drawn("sequenceDiagram\n  Alice->>Bob: Hi\n  Bob-->>Alice: Hello"),
        [
            "+-------+  +-----+",
            "| Alice |  | Bob |",
            "+---+---+  +--+--+",
            "    |   Hi    |   ",
            "    |-------->|   ",
            "    |  Hello  |   ",
            "    |<........|   ",
            "+---+---+  +--+--+",
            "| Alice |  | Bob |",
            "+-------+  +-----+"
        ]
        .join("\n")
    );
    // A tab reads as a space.
    assert_eq!(
        drawn("sequenceDiagram\n\tAlice->>Bob:\tHi\n\tBob-->>Alice: Hello"),
        drawn("sequenceDiagram\n  Alice->>Bob: Hi\n  Bob-->>Alice: Hello")
    );
    // Through the Mermaid reader too, which picks the diagram type from the first statement.
    assert_eq!(
        mermaid("%% greeting\nsequenceDiagram; A->>B: hi")
            .unwrap()
            .text(),
        [
            "+---+  +---+",
            "| A |  | B |",
            "+-+-+  +-+-+",
            "  |  hi  |  ",
            "  |----->|  ",
            "+-+-+  +-+-+",
            "| A |  | B |",
            "+---+  +---+"
        ]
        .join("\n")
    );
}

#[test]
fn every_arrow_type_aliases_autonumber_and_self_messages_are_drawn() {
    let source = "sequenceDiagram
  participant C as Client
  actor S as Server
  autonumber
  C->>+S: GET /
  S->>S: think
  S--x-C
  C-)S: ping
  S--)C: pong
  C<<->>S: both
  C<<-->>C: self
  C->S: plain
  C-->S";
    assert_eq!(
        drawn(source),
        [
            "+--------+        +--------+       ",
            "| Client |        | Server |       ",
            "+---+----+        +---+----+       ",
            "    |    1. GET /     |            ",
            "    |---------------->|            ",
            "    |                 |--+ 2. think",
            "    |                 |  |         ",
            "    |                 |<-+         ",
            "    |       3.        |            ",
            "    |x................|            ",
            "    |     4. ping     |            ",
            "    |----------------)|            ",
            "    |     5. pong     |            ",
            "    |(................|            ",
            "    |     6. both     |            ",
            "    |<--------------->|            ",
            "    |<.+ 7. self      |            ",
            "    |  :              |            ",
            "    |<.+              |            ",
            "    |    8. plain     |            ",
            "    |-----------------|            ",
            "    |       9.        |            ",
            "    |.................|            ",
            "+---+----+        +---+----+       ",
            "| Client |        | Server |       ",
            "+--------+        +--------+       "
        ]
        .join("\n")
    );
    // Activations draw nothing, so a diagram with them matches one without.
    assert_eq!(
        drawn("sequenceDiagram\n  activate A\n  A->>A: me\n  deactivate A"),
        [
            "+---+    ",
            "| A |    ",
            "+-+-+    ",
            "  |--+ me",
            "  |  |   ",
            "  |<-+   ",
            "+-+-+    ",
            "| A |    ",
            "+---+    "
        ]
        .join("\n")
    );
    // A participant with no messages still has its boxes and a lifeline.
    assert_eq!(
        drawn("sequenceDiagram\n  participant A"),
        [
            "+---+", "| A |", "+-+-+", "  |  ", "+-+-+", "| A |", "+---+"
        ]
        .join("\n")
    );
}

#[test]
fn notes_sit_beside_over_or_across_lifelines() {
    let source = "sequenceDiagram
  Note left of A: left
  NOTE RIGHT OF A: right
  note over A: over one
  A->>B: x
  Note over B,A: across both of them here
  Note over B: o";
    assert_eq!(
        drawn(source),
        [
            "       +---+                  +---+",
            "       | A |                  | B |",
            "       +-+-+                  +-+-+",
            "+------+ |                      |  ",
            "| left | |                      |  ",
            "+------+ |                      |  ",
            "         | +-------+            |  ",
            "         | | right |            |  ",
            "         | +-------+            |  ",
            "    +----+-----+                |  ",
            "    | over one |                |  ",
            "    +----+-----+                |  ",
            "         |          x           |  ",
            "         |--------------------->|  ",
            "       +-+----------------------+-+",
            "       | across both of them here |",
            "       +-+----------------------+-+",
            "         |                    +-+-+",
            "         |                    | o |",
            "         |                    +-+-+",
            "       +-+-+                  +-+-+",
            "       | A |                  | B |",
            "       +---+                  +---+"
        ]
        .join("\n")
    );
}

#[test]
fn blocks_are_framed_with_their_kind_and_nested_frames_inset() {
    let source = "```mermaid
sequenceDiagram
  loop every minute
    A->>B: tick
    alt ok
      B-->>A: yes
    else not ok
      B-->>A: no
    end
  end
  rect rgb(0,0,0)
  par
  A->>C: x
  and
  A->>B: y
  end
  end
  critical c
  A->>A
  option o
  B->>A
  end
  opt
  break b
  A->>B
  end
  end
```";
    assert_eq!(
        drawn(source),
        [
            "    +---+   +---+  +---+    ",
            "    | A |   | B |  | C |    ",
            "    +-+-+   +-+-+  +-+-+    ",
            "+-[loop every minute]------+",
            "|     | tick  |      |     |",
            "|     |------>|      |     |",
            "| +-[alt ok]-------------+ |",
            "| |   |  yes  |      |   | |",
            "| |   |<......|      |   | |",
            "| +-[else not ok]--------+ |",
            "| |   |  no   |      |   | |",
            "| |   |<......|      |   | |",
            "| +----------------------+ |",
            "+--------------------------+",
            "+-[par]--------------------+",
            "|     |   x   |      |     |",
            "|     |------------->|     |",
            "+-[and]--------------------+",
            "|     |   y   |      |     |",
            "|     |------>|      |     |",
            "+--------------------------+",
            "+-[critical c]-------------+",
            "|     |--+    |      |     |",
            "|     |  |    |      |     |",
            "|     |<-+    |      |     |",
            "+-[option o]---------------+",
            "|     |<------|      |     |",
            "+--------------------------+",
            "+-[opt]--------------------+",
            "| +-[break b]------------+ |",
            "| |   |------>|      |   | |",
            "| +----------------------+ |",
            "+--------------------------+",
            "    +-+-+   +-+-+  +-+-+    ",
            "    | A |   | B |  | C |    ",
            "    +---+   +---+  +---+    "
        ]
        .join("\n")
    );
    // A frame widens until its label fits.
    assert_eq!(
        drawn(
            "sequenceDiagram\n  loop a very long loop label that is wider than the content\n  A->>B\n  end"
        ),
        [
            "  +---+  +---+                                                  ",
            "  | A |  | B |                                                  ",
            "  +-+-+  +-+-+                                                  ",
            "+-[loop a very long loop label that is wider than the content]-+",
            "|   |----->|                                                   |",
            "+--------------------------------------------------------------+",
            "  +-+-+  +-+-+                                                  ",
            "  | A |  | B |                                                  ",
            "  +---+  +---+                                                  "
        ]
        .join("\n")
    );
}

#[test]
fn unsupported_or_malformed_statements_are_refused_with_their_line() {
    for (source, error) in [
        ("", "A sequence diagram starts with sequenceDiagram"),
        (
            "%% only a comment",
            "A sequence diagram starts with sequenceDiagram",
        ),
        (
            "graph TD",
            "Line 1: a sequence diagram starts with sequenceDiagram",
        ),
        (
            "sequenceDiagram",
            "A sequence diagram needs at least one participant",
        ),
        (
            "sequenceDiagram\n box Aqua Group",
            "Line 2: box is not supported",
        ),
        (
            "sequenceDiagram\n autonumber 5",
            "Line 2: autonumber is not supported",
        ),
        (
            "sequenceDiagram\n else",
            "Line 2: else is only read inside alt",
        ),
        (
            "sequenceDiagram\n alt\n and",
            "Line 3: and is only read inside par",
        ),
        (
            "sequenceDiagram\n par\n option",
            "Line 3: option is only read inside critical",
        ),
        ("sequenceDiagram\n end", "Line 2: end closes no block"),
        (
            "sequenceDiagram\n A->>B\n loop",
            "The loop block opened on line 3 is not closed",
        ),
        (
            "sequenceDiagram\n hello",
            "Line 2: 'hello' is not a statement this reads",
        ),
        (
            "sequenceDiagram\n A=>B",
            "Line 2: 'A=>B' is not a statement this reads",
        ),
        (
            "sequenceDiagram\n A-|/B: x",
            "Line 2: the arrow in 'A-|/B: x' is not one this reads",
        ),
        (
            "sequenceDiagram\n Note A",
            "Line 2: a note is written Note over A: text",
        ),
        (
            "sequenceDiagram\n Note under A: x",
            "Line 2: a note is placed right of, left of or over a participant",
        ),
        (
            "sequenceDiagram\n Note right of A,B: x",
            "Line 2: a participant is named with ASCII letters, digits and _, not 'A,B'",
        ),
        (
            "sequenceDiagram\n participant John Doe",
            "Line 2: a participant is named with ASCII letters, digits and _, not 'John Doe'",
        ),
        (
            "sequenceDiagram\n A->>B: caf\u{e9}",
            "Line 2: byte 12 is 0xc3, not printable ASCII",
        ),
    ] {
        assert_eq!(refused(source), error, "{source:?}");
    }
}

#[test]
fn sequence_diagrams_stay_within_the_limits() {
    let participants: String = (0..65).map(|i| format!("participant P{i}\n")).collect();
    assert_eq!(
        refused(&format!("sequenceDiagram\n{participants}")),
        "Line 66: a sequence diagram has at most 64 participants"
    );
    let messages = "A->>B\n".repeat(257);
    assert_eq!(
        refused(&format!("sequenceDiagram\n{messages}")),
        "Line 258: a sequence diagram has at most 256 messages, notes and block lines"
    );
    assert_eq!(
        refused(&format!("sequenceDiagram\n{}", "A->>B\n".repeat(251))),
        "The sequence diagram needs 12 columns by 257 rows; the limit is 512 by 256"
    );
    assert_eq!(
        refused(&format!("sequenceDiagram\nA->>B: {}", "x".repeat(510))),
        "The sequence diagram needs 518 columns by 8 rows; the limit is 512 by 256"
    );
    assert_eq!(
        refused(&" ".repeat(16 * 1024 * 1024 + 1)),
        "The sequence diagram exceeds the 16 MiB input limit"
    );
    // A box exactly as wide as the limit still fits.
    let widest = drawn(&format!(
        "sequenceDiagram\nparticipant A as {}",
        "w".repeat(508)
    ));
    assert!(widest.split('\n').all(|row| row.len() == 512), "{widest}");
}

#[test]
fn message_text_sits_beside_its_sender_clear_of_other_lifelines() {
    let source = "sequenceDiagram
  participant A
  participant B
  participant C
  A->>C: abc
  C-->>A: a reply wider than one gap";
    assert_eq!(
        drawn(source),
        [
            "+---+  +---+                        +---+",
            "| A |  | B |                        | C |",
            "+-+-+  +-+-+                        +-+-+",
            "  | abc  |                            |  ",
            "  |---------------------------------->|  ",
            "  |      | a reply wider than one gap |  ",
            "  |<..................................|  ",
            "+-+-+  +-+-+                        +-+-+",
            "| A |  | B |                        | C |",
            "+---+  +---+                        +---+",
        ]
        .join("\n")
    );
}

#[test]
fn entities_are_text_and_as_reads_in_any_case() {
    // `#59;` is a `;` inside the text, not the end of the statement; a `#` that starts no
    // entity stays as written.
    let source = "sequenceDiagram
  participant A AS Alice
  A->>B: first #59; still first; B->>A: C# #quot;q#quot; #35;
  Note over A: #lt;#gt;#amp;#semi;#colon;#num;#apos;#nbsp;.";
    assert_eq!(
        drawn(source),
        [
            "  +-------+               +---+",
            "  | Alice |               | B |",
            "  +---+---+               +-+-+",
            "      | first ; still first |  ",
            "      |-------------------->|  ",
            "      |      C# \"q\" #       |  ",
            "      |<--------------------|  ",
            "+-----+-----+               |  ",
            "| <>&;:#' . |               |  ",
            "+-----+-----+               |  ",
            "  +---+---+               +-+-+",
            "  | Alice |               | B |",
            "  +-------+               +---+",
        ]
        .join("\n")
    );
    // A code Mermaid does not decode, such as a hexadecimal one, stays as written.
    assert!(drawn("sequenceDiagram\n  A->>B: #x3B; #55296;").contains("| #x3B; #55296; |"));
    for entity in ["#9829;", "#heart;"] {
        assert_eq!(
            refused(&format!("sequenceDiagram\n  A->>B: {entity}")),
            format!("Line 2: {entity} is not an entity for a printable ASCII character")
        );
    }
}

#[test]
fn mermaid_reads_the_diagram_type_from_the_first_statement() {
    for (source, error) in [
        (
            "",
            "The text is empty: a Mermaid diagram starts with flowchart, graph, sequenceDiagram, stateDiagram-v2, classDiagram, erDiagram, gantt, pie, gitGraph, timeline, journey or mindmap",
        ),
        (
            "```mermaid\n```",
            "The text is empty: a Mermaid diagram starts with flowchart, graph, sequenceDiagram, stateDiagram-v2, classDiagram, erDiagram, gantt, pie, gitGraph, timeline, journey or mindmap",
        ),
        (
            "\nquadrantChart",
            "Line 2: a Mermaid diagram starts with flowchart, graph, sequenceDiagram, stateDiagram-v2, classDiagram, erDiagram, gantt, pie, gitGraph, timeline, journey or mindmap",
        ),
        (
            "quadrantChart",
            "Line 1: a Mermaid diagram starts with flowchart, graph, sequenceDiagram, stateDiagram-v2, classDiagram, erDiagram, gantt, pie, gitGraph, timeline, journey or mindmap",
        ),
        // A bad byte or fence is reported before the type, with its line.
        ("caf\u{e9}", "Line 1: byte 4 is 0xc3, not printable ASCII"),
        ("```rust\n```", "Line 1: only a mermaid code block is read"),
        (
            "```mermaid",
            "The code block opened on line 1 is not closed",
        ),
        ("```\n```\nmore", "Line 3: text after the code block"),
        // Other text keeps the flowchart reader's own error.
        (
            "A-->B",
            "Line 1: a flowchart starts with flowchart or graph and a direction",
        ),
    ] {
        assert_eq!(mermaid(source).unwrap_err(), error, "{source:?}");
    }
    assert_eq!(
        mermaid("%% c\nflowchart LR\nA-->B").unwrap().text(),
        "+---+ +---+\n| A +>| B |\n+---+ +---+"
    );
    assert_eq!(
        mermaid("graph TD; A-->B").unwrap().text(),
        "+---+\n| A |\n+-+-+\n  v  \n+---+\n| B |\n+---+"
    );
}
