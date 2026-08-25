//! 集成测试：真实生物课件 .enbx 解析。
//!
//! 样本：`C:\EN5\第二节 植物细胞 李得平 - 副本.enbx`（35 张幻灯片）。
//! 文件不存在时测试 **skip**（打印提示，不 fail），保证 CI 无样本也能通过；
//! 存在时断言核心 Phase 1 覆盖：35 张幻灯片、≥20 张含 Picture、≥5 张含 Shape。

use std::io::Write;
use std::path::Path;

use drafftink_core::model::{CoursewareDoc, Element};

/// 真实生物课件路径（本地样本；缺失则跳过）。
const BIOLOGY_COURSEWARE: &str = r"C:\EN5\第二节 植物细胞 李得平 - 副本.enbx";

fn import(path: &Path) -> Option<(CoursewareDoc, enbx_importer::ImportReport)> {
    enbx_importer::import_enbx(path, None)
        .map_err(|e| eprintln!("[enbx-importer-test] import failed: {e}"))
        .ok()
}

/// 用户断言 1：35 张幻灯片都被读取。
#[test]
fn biology_courseware_reads_all_35_slides() {
    let path = Path::new(BIOLOGY_COURSEWARE);
    if !path.exists() {
        eprintln!("skip: 生物课件不存在（{}），跳过真实样本断言", path.display());
        return;
    }
    let (doc, report) = import(path).expect("生物课件应能成功导入");
    assert_eq!(
        doc.pages.len(),
        35,
        "生物课件应含 35 张幻灯片，实际 {}（report: {report:?}）",
        doc.pages.len()
    );
}

/// 用户断言 2：至少 20 张幻灯片含 Picture 元素。
#[test]
fn biology_courseware_at_least_20_pictures() {
    let path = Path::new(BIOLOGY_COURSEWARE);
    if !path.exists() {
        eprintln!("skip: 生物课件不存在，跳过 Picture 断言");
        return;
    }
    let (doc, _) = import(path).expect("import");
    let slides_with_picture = doc
        .pages
        .iter()
        .filter(|p| p.elements.iter().any(|e| matches!(e, Element::Image(_))))
        .count();
    assert!(
        slides_with_picture >= 20,
        "至少 20 张幻灯片应含 Picture 元素，实际 {slides_with_picture}"
    );
}

/// 用户断言 3：至少 5 张幻灯片含 Shape 元素。
#[test]
fn biology_courseware_at_least_5_shapes() {
    let path = Path::new(BIOLOGY_COURSEWARE);
    if !path.exists() {
        eprintln!("skip: 生物课件不存在，跳过 Shape 断言");
        return;
    }
    let (doc, _) = import(path).expect("import");
    let slides_with_shape = doc
        .pages
        .iter()
        .filter(|p| {
            p.elements.iter().any(|e| {
                // 形状类元素：纯 Shape（PresetGeometry 无 Path）或 SvgShape
                // （CustomGeometry/带 Path → 曲线保真渲染，也算形状）。
                matches!(e, Element::Shape(_)) || matches!(e, Element::SvgShape(_))
            })
        })
        .count();
    assert!(
        slides_with_shape >= 5,
        "至少 5 张幻灯片应含 Shape/SvgShape 元素，实际 {slides_with_shape}"
    );
}

/// 基础解析（不依赖外部文件）：构造一个 3 张幻灯片 + 1 Picture + 1 Shape 的合成
/// .enbx，验证 Board/Reference/Slide 的框架解析与 Picture/Shape 转换链路。
#[test]
fn synthetic_enbx_basic_parse() {
    let dir = std::env::temp_dir();
    let path = dir.join("enbx_importer_synthetic_test.enbx");

    // ── 构造 ZIP：Board + Reference + 3 张 Slide + 1 张真实 PNG 资源。 ──
    let png = make_1x1_png();
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    zip.start_file("[Content_Types].xml", opts).unwrap();
    zip.write_all(b"<?xml version=\"1.0\"?><Types><Default Extension=\"png\" ContentType=\"\"/></Types>").unwrap();

    zip.start_file("Board.xml", opts).unwrap();
    zip.write_all(
        br#"<Board><SlideWidth>1280</SlideWidth><SlideHeight>720</SlideHeight>
           <Slides><Item>slide-a</Item><Item>slide-b</Item><Item>slide-c</Item></Slides>
           <ThemeForBoard><ThemeId>-1</ThemeId></ThemeForBoard></Board>"#,
    )
    .unwrap();

    zip.start_file("Reference.xml", opts).unwrap();
    zip.write_all(
        br#"<Reference><Relationships>
           <Relationship><Id>res-img</Id><Target>Resources/res-img.png</Target><Hash>abc</Hash></Relationship>
        </Relationships></Reference>"#,
    )
    .unwrap();

    // 3 张幻灯片：第 1 张 = 图片；第 2 张 = 形状；第 3 张 = 空（背景色）。
    let bodies: [&[u8]; 3] = [
        // 图片：X/Y/Width/Height 画布位置 + Source。
        br#"<Slide><Id>slide-a</Id><Width>1280</Width><Height>720</Height>
             <Background><ColorBrush>#FF335577</ColorBrush></Background>
             <Elements><Picture>
               <Source>id://res-img</Source>
               <Alpha>1</Alpha>
               <DisplayRegion><Rectangle>0,0,64,64</Rectangle></DisplayRegion>
               <Id>e1</Id><X>10</X><Y>20</Y><Width>300</Width><Height>200</Height><Rotation>0</Rotation>
             </Picture></Elements>
           </Slide>"#
        .as_slice(),
        // 形状：Rectangle + 归一化 Path + 填充/描边。
        br#"<Slide><Id>slide-b</Id><Width>1280</Width><Height>720</Height>
             <Background><ColorBrush>#FFFFFFFF</ColorBrush></Background>
             <Elements><Shape>
               <Geometry><PresetGeometry><GeometryType>Rectangle</GeometryType></PresetGeometry></Geometry>
               <Path>M0,0L1,0 1,1 0,1 0,0z</Path>
               <Background><ColorBrush>#FFFF8800</ColorBrush></Background>
               <Foreground><ColorBrush>#FF000000</ColorBrush></Foreground>
               <Thickness>2</Thickness>
               <Id>e2</Id><X>0</X><Y>0</Y><Width>500</Width><Height>164</Height><Rotation>0</Rotation>
             </Shape></Elements>
           </Slide>"#
        .as_slice(),
        // 空 slide：只有背景。
        br#"<Slide><Id>slide-c</Id><Width>1280</Width><Height>720</Height>
             <Background><ColorBrush>#FF112233</ColorBrush></Background>
             <Elements/></Slide>"#
        .as_slice(),
    ];
    for (i, body) in bodies.iter().enumerate() {
        zip.start_file(format!("Slides/Slide_{i}.xml"), opts).unwrap();
        zip.write_all(body).unwrap();
    }

    zip.start_file("Resources/res-img.png", opts).unwrap();
    zip.write_all(&png).unwrap();
    let buf = zip.finish().unwrap().into_inner();
    std::fs::write(&path, buf).unwrap();

    // ── 解析断言。 ──
    let (doc, report) = enbx_importer::import_enbx(&path, None).expect("synthetic import");
    assert_eq!(doc.pages.len(), 3, "3 张幻灯片（report: {report:?}）");

    // 第 1 张：1 个 Image（真实 ENBX 用 <Picture> 标签，分发必须命中）。
    let imgs = doc.pages[0]
        .elements
        .iter()
        .filter(|e| matches!(e, Element::Image(_)))
        .count();
    assert_eq!(imgs, 1, "slide 0 应含 1 个 Image");
    // 第 2 张：1 个形状类元素（带 Path → SvgShape 保真转换）。
    let shapes = doc.pages[1]
        .elements
        .iter()
        .filter(|e| matches!(e, Element::Shape(_)) || matches!(e, Element::SvgShape(_)))
        .count();
    assert_eq!(shapes, 1, "slide 1 应含 1 个形状（Shape/SvgShape）");
    // 第 3 张：空。
    assert!(doc.pages[2].elements.is_empty(), "slide 2 应为空");

    let _ = std::fs::remove_file(&path);
}

/// 1×1 红色 PNG（image crate 生成，验证图片解码链路）。
fn make_1x1_png() -> Vec<u8> {
    let img: image::RgbaImage = image::ImageBuffer::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]));
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png)
        .expect("png encode");
    buf.into_inner()
}
