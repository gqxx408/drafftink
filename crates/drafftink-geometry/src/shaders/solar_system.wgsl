// ════════════════════════════════════════════════════════════════════════════
// 太阳系 / 地球渲染管线 WGSL 参考着色器
// ────────────────────────────────────────────────────────────────────────────
//
// 本文件为「太阳系 / 地球」3D 渲染管线的参考 WGSL 着色器。
// 当前项目使用 egui::Painter 进行 2D 几何渲染（见 renderer.rs），
// 本着色器记录了未来迁移到「直接 wgpu 渲染管线」时的理想渲染方案。
//
// 渲染特性：
//   1. 顶点着色器：将模型从局部空间变换到世界空间，并传递光照所需向量
//   2. 多层纹理混合：卫星底图 + 叠加层（降水 / 气温 / 人口）+ 法线贴图
//   3. Lambert 漫反射光照模型（含环境光）
//   4. 基于 Fresnel 效应的大气层散射（边缘蓝色辉光）
//
// 说明：此着色器为参考实现，用于文档化未来 wgpu 迁移的技术方案。
//       完整可编译，遵循 WGSL 规范语法。
// ════════════════════════════════════════════════════════════════════════════


// ────────────────────────────────────────────────────────────────────────────
// 第一部分：顶点输入结构
// ────────────────────────────────────────────────────────────────────────────
// 顶点缓冲区传入的每个顶点属性。
// 通过 @location(n) 指定顶点缓冲区中的属性位置索引。
// 对应 Rust 端的 VertexBufferLayout 描述。

struct VertexInput {
    /// 顶点局部空间位置（vec3）
    /// @location(0) 对应顶点缓冲区属性索引 0
    @location(0) position: vec3<f32>,

    /// 顶点法线方向（vec3，已归一化）
    /// @location(1) 对应顶点缓冲区属性索引 1
    @location(1) normal: vec3<f32>,

    /// 纹理坐标（vec2，UV）
    /// @location(2) 对应顶点缓冲区属性索引 2
    @location(2) uv: vec2<f32>,
}


// ────────────────────────────────────────────────────────────────────────────
// 第二部分：顶点输出 / 片段输入结构
// ────────────────────────────────────────────────────────────────────────────
// 顶点着色器输出，经光栅化插值后传入片段着色器。
// 每个字段携带片段着色器所需的几何与纹理信息。

struct VertexOutput {
    /// 裁剪空间位置（GPU 内置，必填）
    /// @builtin(position) 由光栅化器使用，决定像素在屏幕上的位置
    @builtin(position) clip_position: vec4<f32>,

    /// 世界空间位置（vec3）
    /// @location(0) 传递给片段着色器，用于光照与雾效计算
    @location(0) world_position: vec3<f32>,

    /// 世界空间法线（vec3，需在片段中重新归一化）
    /// @location(1) 传递给片段着色器，用于 Lambert 漫反射与 Fresnel 计算
    @location(1) world_normal: vec3<f32>,

    /// 纹理坐标（vec2，UV）
    /// @location(2) 传递给片段着色器，用于采样多层纹理
    @location(2) uv: vec2<f32>,

    /// 视线方向（vec3，从片段指向摄像机，已归一化）
    /// @location(3) 传递给片段着色器，用于 Fresnel 大气散射计算
    @location(3) view_direction: vec3<f32>,
}


// ────────────────────────────────────────────────────────────────────────────
// 第三部分：相机 / 模型 Uniform 缓冲区
// ────────────────────────────────────────────────────────────────────────────
// 绑定组 0（Bind Group 0）：全局相机与模型变换矩阵。
// 每帧由 Rust 端通过 wgpu::Buffer 更新。

struct CameraUniform {
    /// 相机视图-投影组合矩阵（mat4x4<f32>）
    /// view_projection = projection * view
    /// 将世界空间坐标变换到裁剪空间
    view_projection: mat4x4<f32>,

    /// 模型矩阵（mat4x4<f32>）
    /// 将局部空间坐标变换到世界空间
    /// 包含平移、旋转、缩放（地球自转与公转通过此矩阵实现）
    model_matrix: mat4x4<f32>,

    /// 光照方向（vec3，世界空间，已归一化）
    /// 指向光源的方向，用于 Lambert 漫反射计算
    light_direction: vec3<f32>,

    /// 摄像机位置（vec3，世界空间）
    /// 用于计算视线方向与 Fresnel 效应
    camera_position: vec3<f32>,
};

// Uniform 缓冲区绑定
// @group(0) @binding(0)：绑定组 0，槽位 0
@group(0) @binding(0) var<uniform> camera: CameraUniform;


// ────────────────────────────────────────────────────────────────────────────
// 第四部分：纹理与采样器绑定
// ────────────────────────────────────────────────────────────────────────────
// 绑定组 1（Bind Group 1）：纹理资源绑定。
// 包含基础卫星纹理、多个叠加层纹理、法线贴图及采样器。

// 基础纹理：卫星影像（地球表面真实纹理）
// @group(1) @binding(0)：绑定组 1，槽位 0
@group(1) @binding(0) var base_texture: texture_2d<f32>;

// 叠加纹理 - 降水（降水量分布图）
// @group(1) @binding(1)：绑定组 1，槽位 1
@group(1) @binding(1) var rainfall_texture: texture_2d<f32>;

// 叠加纹理 - 气温（温度分布图）
// @group(1) @binding(2)：绑定组 1，槽位 2
@group(1) @binding(2) var temperature_texture: texture_2d<f32>;

// 叠加纹理 - 人口（人口密度分布图）
// @group(1) @binding(3)：绑定组 1，槽位 3
@group(1) @binding(3) var population_texture: texture_2d<f32>;

// 法线贴图（用于表面细节光照增强）
// @group(1) @binding(4)：绑定组 1，槽位 4
@group(1) @binding(4) var normal_map: texture_2d<f32>;

// 线性采样器（用于纹理过滤）
// @group(1) @binding(5)：绑定组 1，槽位 5
@group(1) @binding(5) var texture_sampler: sampler;


// ────────────────────────────────────────────────────────────────────────────
// 第五部分：混合参数 Uniform 缓冲区
// ────────────────────────────────────────────────────────────────────────────
// 绑定组 2（Bind Group 2）：纹理混合控制参数。
// 控制基础纹理与叠加层纹理的混合权重及当前激活的叠加层。

struct BlendUniform {
    /// 基础纹理混合因子（f32）
    /// 控制卫星底图的贡献权重
    base_blend_factor: f32,

    /// 叠加纹理混合因子（f32）
    /// 控制当前叠加层的贡献权重
    overlay_blend_factor: f32,

    /// 图层模式（u32）
    /// 0 = 仅卫星图（Satellite only）
    /// 1 = 降水叠加（Rainfall）
    /// 2 = 气温叠加（Temperature）
    /// 3 = 人口叠加（Population）
    layer_mode: u32,
};

// Uniform 缓冲区绑定
// @group(2) @binding(0)：绑定组 2，槽位 0
@group(2) @binding(0) var<uniform> blend: BlendUniform;


// ════════════════════════════════════════════════════════════════════════════
// 第六部分：顶点着色器入口
// ════════════════════════════════════════════════════════════════════════════
// 将顶点从局部空间变换到裁剪空间，并计算光照所需的向量传递给片段着色器。

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;

    // ── 1. 计算世界空间位置 ──
    // model_matrix 将局部坐标变换到世界空间
    let world_pos = camera.model_matrix * vec4<f32>(input.position, 1.0);
    output.world_position = world_pos.xyz;

    // ── 2. 计算世界空间法线 ──
    // 法线变换使用 model_matrix 的上 3x3 部分
    // 注意：若存在非均匀缩放，应使用逆转置矩阵（本简化实现假定均匀缩放）
    let world_normal = camera.model_matrix * vec4<f32>(input.normal, 0.0);
    output.world_normal = world_normal.xyz;

    // ── 3. 传递纹理坐标 ──
    output.uv = input.uv;

    // ── 4. 计算视线方向 ──
    // view_direction = normalize(camera_position - world_position)
    // 方向从片段表面指向摄像机，用于 Fresnel 大气散射计算
    output.view_direction = normalize(camera.camera_position - world_pos.xyz);

    // ── 5. 计算裁剪空间位置 ──
    // view_projection 将世界空间坐标变换到裁剪空间
    output.clip_position = camera.view_projection * world_pos;

    return output;
}


// ════════════════════════════════════════════════════════════════════════════
// 第七部分：片段着色器入口
// ════════════════════════════════════════════════════════════════════════════
// 执行多层纹理混合、Lambert 光照计算与 Fresnel 大气散射，输出最终像素颜色。

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {

    // ────────────────────────────────────────────────────────────────────────
    // 7.1 多层纹理采样与混合
    // ────────────────────────────────────────────────────────────────────────
    // 根据图层模式（layer_mode）选择对应的叠加层纹理，
    // 与基础卫星纹理按混合因子进行线性混合。
    //
    // 混合公式：
    //   final_color = base_texture_sample * base_blend_factor
    //               + overlay_texture_sample * overlay_blend_factor

    // 采样基础卫星纹理
    let base_color = textureSample(base_texture, texture_sampler, input.uv);

    // 根据图层模式采样对应的叠加层纹理
    var overlay_color = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    switch blend.layer_mode {
        case 0u: {
            // 模式 0：仅卫星图（不叠加任何图层）
            overlay_color = vec4<f32>(0.0, 0.0, 0.0, 0.0);
        }
        case 1u: {
            // 模式 1：降水叠加层
            overlay_color = textureSample(rainfall_texture, texture_sampler, input.uv);
        }
        case 2u: {
            // 模式 2：气温叠加层
            overlay_color = textureSample(temperature_texture, texture_sampler, input.uv);
        }
        case 3u: {
            // 模式 3：人口叠加层
            overlay_color = textureSample(population_texture, texture_sampler, input.uv);
        }
        default: {
            // 默认：无叠加
            overlay_color = vec4<f32>(0.0, 0.0, 0.0, 0.0);
        }
    }

    // 多层纹理线性混合
    // 最终颜色 = 基础纹理 * 基础因子 + 叠加纹理 * 叠加因子
    var blended_color = base_color.rgb * blend.base_blend_factor
                      + overlay_color.rgb * blend.overlay_blend_factor;


    // ────────────────────────────────────────────────────────────────────────
    // 7.2 法线贴图增强（表面细节光照）
    // ────────────────────────────────────────────────────────────────────────
    // 从法线贴图采样并解码切线空间法线，叠加到几何法线上，
    // 为地球表面增加地形起伏的细节光照效果。

    // 采样法线贴图（RGB 编码的法线方向，范围 [0,1]）
    let sampled_normal = textureSample(normal_map, texture_sampler, input.uv).rgb;

    // 解码法线：从 [0, 1] 映射到 [-1, 1]
    let detail_normal = sampled_normal * 2.0 - 1.0;

    // 重新归一化几何法线（光栅化插值后可能不再单位化）
    var normal = normalize(input.world_normal);

    // 将法线贴图细节叠加到几何法线（简化版：直接加和并归一化）
    // 注：严格实现需构建 TBN 矩阵进行切线空间变换
    normal = normalize(normal + detail_normal * 0.3);


    // ────────────────────────────────────────────────────────────────────────
    // 7.3 Lambert 漫反射光照
    // ────────────────────────────────────────────────────────────────────────
    // 经典 Lambert 漫反射模型，包含环境光项。
    //
    // 计算公式：
    //   ambient       = 0.35                              （环境光常数）
    //   diffuse       = max(0, dot(normal, light_dir))    （漫反射强度）
    //   brightness    = ambient + (1.0 - ambient) * diffuse
    //   final_color   = blended_color * brightness

    // 归一化光照方向（确保点积准确）
    let light_dir = normalize(camera.light_direction);

    // 环境光常数（背光面的最低亮度，防止完全黑）
    let ambient = 0.35;

    // Lambert 漫反射：法线与光照方向的点积
    // max(0, ...) 确保背光面（法线背向光源）贡献为零
    let diffuse = max(0.0, dot(normal, light_dir));

    // 综合亮度 = 环境光 + 漫反射贡献
    // (1.0 - ambient) 将漫反射范围从 [0, 1] 缩放到 [0, 0.65]
    let brightness = ambient + (1.0 - ambient) * diffuse;

    // 将亮度应用于混合后的纹理颜色
    blended_color = blended_color * brightness;


    // ────────────────────────────────────────────────────────────────────────
    // 7.4 大气层散射（Fresnel 效应）
    // ────────────────────────────────────────────────────────────────────────
    // 基于 Fresnel 效应模拟地球大气层的边缘辉光。
    // 当视线方向与表面法线夹角越大（边缘），大气散射越强，产生蓝色光晕。
    //
    // 计算公式：
    //   fresnel          = pow(1.0 - max(0, dot(view_dir, normal)), 3.0)
    //   atmosphere_color = vec3(0.3, 0.6, 1.0)         （蓝色辉光）
    //   atmosphere_alpha = fresnel * 0.5                （散射强度）
    //   final_color     += atmosphere_color * atmosphere_alpha

    // 重新归一化视线方向（插值后可能不再单位化）
    let view_dir = normalize(input.view_direction);

    // Fresnel 项：视线与法线夹角越大（越靠近边缘），值越大
    // max(0, dot(...)) 防止负值
    // pow(..., 3.0) 使边缘辉光更集中、过渡更锐利
    let fresnel = pow(1.0 - max(0.0, dot(view_dir, normal)), 3.0);

    // 大气层颜色：蓝色辉光（模拟地球大气瑞利散射）
    let atmosphere_color = vec3<f32>(0.3, 0.6, 1.0);

    // 大气散射透明度：fresnel 越强，散射越强，最大不超过 0.5
    let atmosphere_alpha = fresnel * 0.5;

    // 将大气辉光叠加到最终颜色
    // 边缘处蓝色增强，中心处几乎无影响
    blended_color = blended_color + atmosphere_color * atmosphere_alpha;


    // ────────────────────────────────────────────────────────────────────────
    // 7.5 输出最终颜色
    // ────────────────────────────────────────────────────────────────────────
    // 输出 RGBA 颜色，Alpha 通道固定为 1.0（完全不透明）。
    // 输出到 @location(0)，对应渲染目标的第一个颜色附件。

    return vec4<f32>(blended_color, 1.0);
}


// ════════════════════════════════════════════════════════════════════════════
// 附录：Rust 端 wgpu 绑定组布局参考
// ════════════════════════════════════════════════════════════════════════════
//
// 以下为 Rust 端对应的 wgpu 绑定组布局描述，供未来实现参考：
//
// ── 绑定组 0：相机 Uniform ──
// BindGroupLayoutEntry {
//     binding: 0,
//     visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
//     ty: BindingType::Buffer {
//         ty: BufferBindingType::Uniform,
//         has_dynamic_offset: false,
//         min_binding_size: Some(64 + 64 + 16 + 16),  // mat4x4 + mat4x4 + vec3(pad) + vec3(pad)
//     },
// }
//
// ── 绑定组 1：纹理资源 ──
// binding 0: BindingType::Texture { sample_type: Float, view_dimension: D2 }
// binding 1: BindingType::Texture { sample_type: Float, view_dimension: D2 }
// binding 2: BindingType::Texture { sample_type: Float, view_dimension: D2 }
// binding 3: BindingType::Texture { sample_type: Float, view_dimension: D2 }
// binding 4: BindingType::Texture { sample_type: Float, view_dimension: D2 }
// binding 5: BindingType::Sampler { filtering: true }
//
// ── 绑定组 2：混合参数 Uniform ──
// BindGroupLayoutEntry {
//     binding: 0,
//     visibility: ShaderStages::FRAGMENT,
//     ty: BindingType::Buffer {
//         ty: BufferBindingType::Uniform,
//         has_dynamic_offset: false,
//         min_binding_size: Some(16),  // f32 + f32 + u32 + padding
//     },
// }
//
// ════════════════════════════════════════════════════════════════════════════
