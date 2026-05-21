use granit_parser::{Comment, Marker, Span};

#[test]
fn comment_preserves_raw_text_and_exposes_trimmed_text() {
    let span = Span::new(
        Marker::new(0, 1, 0).with_byte_offset(Some(0)),
        Marker::new(11, 1, 11).with_byte_offset(Some(11)),
    );
    let comment = Comment::new(span, " payload ");

    assert_eq!(comment.span, span);
    assert_eq!(comment.text, " payload ");
    assert_eq!(comment.trimmed_text(), "payload");
}

#[test]
fn comment_preserves_single_space_payload() {
    let span = Span::new(
        Marker::new(0, 1, 0).with_byte_offset(Some(0)),
        Marker::new(2, 1, 2).with_byte_offset(Some(2)),
    );
    let comment = Comment::new(span, " ");

    assert_eq!(comment.text, " ");
    assert_eq!(comment.trimmed_text(), "");
}
