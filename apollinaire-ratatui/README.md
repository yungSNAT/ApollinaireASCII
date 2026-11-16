# apollinaire-ratatui

[ApollinaireASCII](https://github.com/yungSNAT/ApollinaireASCII) lettering and art as a [ratatui](https://ratatui.rs) widget. `Ascii` draws lettering in any of the 24 bundled FIGlet and TOIlet fonts, a catalogue piece or any frame you build with the library, aligned left, centred or right, in a style you choose. Every cell it writes is printable ASCII.

```rust
use apollinaire_ratatui::Ascii;
use ratatui::{layout::Alignment, style::Stylize};

fn draw(frame: &mut ratatui::Frame) -> Result<(), String> {
    let title = Ascii::lettering("Hello", "slant")?
        .alignment(Alignment::Center)
        .style(ratatui::style::Style::new().cyan());
    frame.render_widget(&title, frame.area());
    Ok(())
}
```

`Ascii::size` gives the columns and rows the art needs, for a layout constraint. A frame larger than its area is clipped on the right and bottom.

It depends on `ratatui-core` 0.1, the widget interface of ratatui 0.30. MIT licence.
