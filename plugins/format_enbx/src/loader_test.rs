#[cfg(test)]
mod tests {
    use std::io::Write;

    use crate::loader::load_enbx;
    use drafftink_core::model::Element;
    use drafftink_core::plugin::api::DummyContext;

    /// Build a minimal .enbx ZIP in memory and verify the full pipeline.
    #[test]
    fn load_minimal_enbx_zip() {
        let mut buf = Vec::new();
        {
            let mut z = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts = zip::write::FileOptions::default();

            // Board.xml
            z.start_file("Board.xml", opts).unwrap();
            z.write_all(
                b"<Board><SlideWidth>1280</SlideWidth><SlideHeight>720</SlideHeight><Slides><Item>x</Item></Slides></Board>",
            ).unwrap();

            // Slide
            z.start_file("Slides/Slide_0.xml", opts).unwrap();
            z.write_all(
                br#"<?xml version="1.0" encoding="utf-8"?><Slide xmlns="http://schemas.seewo.com/enbx/2016"><Id>a</Id><Width>1280</Width><Height>720</Height><Elements><Element><Id>x1</Id><X>100</X><Y>200</Y><Width>300</Width><Height>50</Height><Rotation>0</Rotation><IsLocked>False</IsLocked><Background><ColorBrush>#00FFFFFF</ColorBrush></Background><Text><RichText><TextRuns><TextRun><Text>Hello</Text><FontSize>24</FontSize><FontFamily><Source>Arial</Source></FontFamily><FontWeight>Normal</FontWeight><Foreground><ColorBrush>#FF000000</ColorBrush></Foreground></TextRun></TextRuns></RichText></Text></Element></Elements></Slide>"#,
            ).unwrap();

            z.finish().unwrap();
        }

        let ctx = DummyContext;
        let doc = load_enbx(&buf, &ctx).expect("load");

        assert_eq!(doc.pages.len(), 1);
        assert_eq!(doc.page_size, [1280.0, 720.0]);

        let page = &doc.pages[0];
        assert_eq!(page.elements.len(), 1);

        if let Element::Text(t) = &page.elements[0] {
            assert_eq!(t.text, "Hello");
            assert_eq!(t.font_size, 24.0);
            assert_eq!(t.font_family, "Arial");
            assert_eq!(t.base.position, [100.0, 200.0]);
            assert_eq!(t.base.size, [300.0, 50.0]);
        } else {
            panic!("Expected Text element");
        }
    }
}
