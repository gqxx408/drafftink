#[cfg(test)]
mod tests {
    use super::super::parser::{parse_board, parse_document, parse_slide_xml};
    use super::super::elements::{
        shape::SlideElement,
        text::ArgbColor,
    };

    const TEST_SLIDE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?><Slide xmlns="http://schemas.seewo.com/enbx/2016"><Id>69d1f83b77124f95bdf330bf28bddfb1</Id><Width>1280</Width><Height>720</Height><Background><ColorBrush>#FFFFFFFF</ColorBrush></Background><Elements><Element><Id>964771d35d3644519dbb9b1a9733f896</Id><X>278.377</X><Y>270.752</Y><Width>710</Width><Height>107.793</Height><Rotation>0</Rotation><IsLocked>False</IsLocked><Background><ColorBrush>#00FFFFFF</ColorBrush></Background><Text><RichText><TextRuns><TextRun><Text>HI,SEEWO</Text><FontSize>40</FontSize><FontFamily><Source>微软雅黑</Source></FontFamily><FontWeight>Normal</FontWeight><Foreground><ColorBrush>#FF000000</ColorBrush></Foreground></TextRun></TextRuns></RichText></Text></Element></Elements></Slide>"#;

    #[test]
    fn parse_single_text_element() {
        let elements = parse_slide_xml(TEST_SLIDE_XML).expect("Parse should succeed");
        assert_eq!(elements.len(), 1);

        let text = match &elements[0] {
            SlideElement::Text(t) => t,
            _ => panic!("Expected Text element"),
        };

        assert_eq!(text.id, "964771d35d3644519dbb9b1a9733f896");
        assert!((text.x - 278.377).abs() < 0.001, "x = {}", text.x);
        assert!((text.y - 270.752).abs() < 0.001, "y = {}", text.y);
        assert_eq!(text.width, 710.0);
        assert!((text.height - 107.793).abs() < 0.001);
        assert_eq!(text.rotation, 0.0);
        assert_eq!(text.is_locked, false);

        assert_eq!(text.background.a, 0);
        assert_eq!(text.background.r, 255);
        assert_eq!(text.background.g, 255);
        assert_eq!(text.background.b, 255);

        assert_eq!(text.content, "HI,SEEWO");
        assert_eq!(text.font_size, 40.0);
        assert_eq!(text.font_family, "微软雅黑");
        assert_eq!(text.font_weight, "Normal");

        assert_eq!(text.foreground.a, 255);
        assert_eq!(text.foreground.r, 0);
        assert_eq!(text.foreground.g, 0);
        assert_eq!(text.foreground.b, 0);

        assert_eq!(text.foreground.to_rgba(), [0, 0, 0, 255]);
        assert_eq!(text.background.to_rgba(), [255, 255, 255, 0]);
    }

    #[test]
    fn argb_color_from_hex() {
        let c = ArgbColor::from_hex("#FF000000").unwrap();
        assert_eq!(c.to_rgba(), [0, 0, 0, 255]);

        let c = ArgbColor::from_hex("#FFFFFFFF").unwrap();
        assert_eq!(c.to_rgba(), [255, 255, 255, 255]);

        let c = ArgbColor::from_hex("#80FF0000").unwrap();
        assert_eq!(c.a, 128);
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 0);
        assert_eq!(c.b, 0);

        assert!(ArgbColor::from_hex("FF000000").is_err());
        assert!(ArgbColor::from_hex("#FF00").is_err());
    }

    #[test]
    fn empty_elements_returns_empty_vec() {
        let xml = r#"<Slide xmlns="http://schemas.seewo.com/enbx/2016"><Elements></Elements></Slide>"#;
        let elements = parse_slide_xml(xml).unwrap();
        assert!(elements.is_empty());
    }

    /// Real .enbx files have the default namespace.
    #[test]
    fn parse_slide_xml_with_default_namespace() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<Slide xmlns="http://schemas.seewo.com/enbx/2016">
  <Id>slide_x</Id>
  <Width>1280</Width>
  <Height>720</Height>
  <Background><ColorBrush>#FFFFFFFF</ColorBrush></Background>
  <Elements>
    <Element>
      <Id>ns-test-1</Id>
      <X>278.377</X>
      <Y>270.752</Y>
      <Width>710</Width>
      <Height>107.793</Height>
      <Rotation>0</Rotation>
      <IsLocked>False</IsLocked>
      <Background><ColorBrush>#00FFFFFF</ColorBrush></Background>
      <Text>
        <RichText><TextRuns><TextRun>
          <Text>NAMESPACE_OK</Text>
          <FontSize>40</FontSize>
          <FontFamily><Source>微软雅黑</Source></FontFamily>
          <FontWeight>Normal</FontWeight>
          <Foreground><ColorBrush>#FF000000</ColorBrush></Foreground>
        </TextRun></TextRuns></RichText>
      </Text>
    </Element>
  </Elements>
</Slide>"#;
        let elements = parse_slide_xml(xml).expect("Parse with namespace should succeed");
        assert_eq!(elements.len(), 1, "Should parse 1 text element");
        let text = match &elements[0] {
            SlideElement::Text(t) => t,
            _ => panic!("Expected Text"),
        };
        assert_eq!(text.content, "NAMESPACE_OK");
        assert_eq!(text.id, "ns-test-1");
    }

    // ── parse_board tests ────────────────────────────────────────

    #[test]
    fn parse_board_extracts_dimensions() {
        let xml = r#"<Board><SlideWidth>1280</SlideWidth><SlideHeight>720</SlideHeight></Board>"#;
        let (w, h) = parse_board(xml).expect("parse_board should succeed");
        assert_eq!(w, 1280.0);
        assert_eq!(h, 720.0);
    }

    #[test]
    fn parse_board_with_namespace() {
        let xml = r#"<?xml version="1.0"?><Board xmlns="http://schemas.seewo.com/enbx/2016"><SlideWidth>1920</SlideWidth><SlideHeight>1080</SlideHeight></Board>"#;
        let (w, h) = parse_board(xml).expect("parse_board with namespace should succeed");
        assert_eq!(w, 1920.0);
        assert_eq!(h, 1080.0);
    }

    #[test]
    fn parse_board_defaults() {
        let xml = r#"<Board></Board>"#;
        let (w, h) = parse_board(xml).expect("parse_board should succeed");
        assert_eq!(w, 1920.0);
        assert_eq!(h, 1080.0);
    }

    // ── parse_document tests ─────────────────────────────────────

    #[test]
    fn parse_document_extracts_metadata() {
        let xml = r#"<Document><Title>数学课件</Title><Author>张老师</Author></Document>"#;
        let (title, author) = parse_document(xml);
        assert_eq!(title, "数学课件");
        assert_eq!(author, "张老师");
    }

    #[test]
    fn parse_document_with_namespace() {
        let xml = r#"<?xml version="1.0"?><Document xmlns="http://schemas.seewo.com/enbx/2016"><Title>NS_TITLE</Title><Author>NS_AUTHOR</Author></Document>"#;
        let (title, author) = parse_document(xml);
        assert_eq!(title, "NS_TITLE");
        assert_eq!(author, "NS_AUTHOR");
    }

    #[test]
    fn parse_document_empty() {
        let xml = r#"<Document></Document>"#;
        let (title, author) = parse_document(xml);
        assert_eq!(title, "");
        assert_eq!(author, "");
    }
}
