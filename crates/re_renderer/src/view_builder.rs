use anyhow::{Context, Ok};
use parking_lot::RwLock;
use std::sync::Arc;

use crate::{
    context::*,
    global_bindings::FrameUniformBuffer,
    renderer::{tonemapper::*, Drawable, Renderer},
    resource_pools::{
        bind_group_pool::BindGroupHandle,
        buffer_pool::{BufferDesc, BufferHandle},
        texture_pool::*,
    },
};

type DrawFn = dyn for<'a, 'b> Fn(&'b RenderContext, &'a mut wgpu::RenderPass<'b>) -> anyhow::Result<()>
    + Sync
    + Send;

struct QueuedDraw {
    draw_func: Box<DrawFn>,
    sorting_index: u32,
}

/// The highest level rendering block in `re_renderer`.
/// Used to build up/collect various resources and then send them off for rendering of  a single view.
#[derive(Default)]
pub struct ViewBuilder {
    tonemapping_draw_data: TonemapperDrawable,

    bind_group_0: BindGroupHandle,

    frame_uniform_buffer: BufferHandle,
    hdr_render_target: TextureHandle,
    depth_buffer: TextureHandle,

    queued_draws: Vec<QueuedDraw>, // &mut wgpu::RenderPass
}

pub type SharedViewBuilder = Arc<RwLock<ViewBuilder>>;

/// Basic configuration for a target view.
pub struct TargetConfiguration {
    pub resolution_in_pixel: [u32; 2],

    pub view_from_world: macaw::IsoTransform,

    pub fov_y: f32,
    pub near_plane_distance: f32,

    /// Every target needs an individual as persistent as possible identifier.
    /// This is used to facilitate easier resource re-use.
    pub target_identifier: u64,
}

impl ViewBuilder {
    pub const FORMAT_HDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
    pub const FORMAT_DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;

    pub fn new() -> Self {
        ViewBuilder {
            tonemapping_draw_data: Default::default(),

            bind_group_0: BindGroupHandle::default(),

            frame_uniform_buffer: BufferHandle::default(),
            hdr_render_target: TextureHandle::default(),
            depth_buffer: TextureHandle::default(),

            queued_draws: Vec::new(),
        }
    }

    pub fn new_shared() -> SharedViewBuilder {
        Arc::new(RwLock::new(ViewBuilder::new()))
    }

    pub fn setup_view(
        &mut self,
        ctx: &mut RenderContext,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        config: &TargetConfiguration,
    ) -> anyhow::Result<&mut Self> {
        // TODO(andreas): Should tonemapping preferences go here as well? Likely!
        // TODO(andreas): How should we treat multisampling. Once we start it we also need to deal with MSAA resolves
        self.hdr_render_target = ctx.resource_pools.textures.request(
            device,
            &render_target_2d_desc(
                Self::FORMAT_HDR,
                config.resolution_in_pixel[0],
                config.resolution_in_pixel[1],
                1,
            ),
        );
        self.depth_buffer = ctx.resource_pools.textures.request(
            device,
            &render_target_2d_desc(
                Self::FORMAT_DEPTH,
                config.resolution_in_pixel[0],
                config.resolution_in_pixel[1],
                1,
            ),
        );

        self.tonemapping_draw_data = TonemapperDrawable::new(ctx, device, self.hdr_render_target);

        // Setup frame uniform buffer
        {
            self.frame_uniform_buffer = ctx.resource_pools.buffers.request(
                device,
                &BufferDesc {
                    label: "frame uniform buffer".into(),
                    size: std::mem::size_of::<FrameUniformBuffer>() as _,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,

                    // We need to make sure that every target gets a different frame uniform buffer.
                    // If we don't do that, frame uniform buffers from different [`ViewBuilder`] might overwrite each other.
                    // (note thought that we do *not* want to hash the current contents of the uniform buffer
                    // because then we'd create a new buffer every frame!)
                    content_id: config.target_identifier,
                },
            );

            let view_from_world = config.view_from_world.to_mat4();

            // We use infinite reverse-z projection matrix.
            // * great precision both with floating point and integer: https://developer.nvidia.com/content/depth-precision-visualized
            // * no need to worry about far plane
            // * 0 depth == near is more intuitive anyway!
            let projection_from_view = glam::Mat4::perspective_infinite_reverse_rh(
                config.fov_y,
                config.resolution_in_pixel[0] as f32 / config.resolution_in_pixel[1] as f32,
                config.near_plane_distance,
            );
            let projection_from_world = projection_from_view * view_from_world;

            let view_from_projection = projection_from_view.inverse();

            // Calculate the top right corner of the screen in view space.
            // Top right corner in projection space is (also called Normalized Device Coordinates) is (1, 1, 0)
            // (z zero means it sits on the near-plane)
            let top_right_screen_corner_in_view = view_from_projection
                .transform_point3(glam::vec3(1.0, 1.0, 0.0))
                .truncate()
                .normalize();

            queue.write_buffer(
                &ctx.resource_pools
                    .buffers
                    .get(self.frame_uniform_buffer)
                    .unwrap()
                    .buffer,
                0,
                bytemuck::bytes_of(&FrameUniformBuffer {
                    view_from_world: glam::Affine3A::from_mat4(view_from_world).into(),
                    projection_from_view: projection_from_view.into(),
                    projection_from_world: projection_from_world.into(),
                    camera_position: config.view_from_world.translation().into(),
                    top_right_screen_corner_in_view: top_right_screen_corner_in_view.into(),
                }),
            );
        }

        self.bind_group_0 = ctx.shared_renderer_data.global_bindings.create_bind_group(
            &mut ctx.resource_pools,
            device,
            self.frame_uniform_buffer,
        );

        Ok(self)
    }

    pub fn queue_draw<D: Drawable + Sync + Send + Clone + 'static>(
        &mut self,
        draw_data: &D,
    ) -> &mut Self {
        let draw_data = draw_data.clone();

        self.queued_draws.push(QueuedDraw {
            draw_func: Box::new(move |ctx, pass| {
                let renderer = ctx
                    .renderers
                    .get::<D::Renderer>()
                    .context("failed to retrieve renderer")?;
                renderer.draw(&ctx.resource_pools, pass, &draw_data)
            }),
            sorting_index: D::Renderer::draw_order(),
        });

        self
    }

    /// Draws the frame as instructed to a temporary HDR target.
    pub fn draw(
        &mut self,
        ctx: &RenderContext,
        encoder: &mut wgpu::CommandEncoder,
    ) -> anyhow::Result<()> {
        let color = ctx
            .resource_pools
            .textures
            .get(self.hdr_render_target)
            .context("hdr render target")?;
        let depth = ctx
            .resource_pools
            .textures
            .get(self.depth_buffer)
            .context("depth buffer")?;

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("frame builder hdr pass"), // TODO(andreas): It would be nice to specify this from the outside so we know which view we're rendering
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color.default_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: true,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth.default_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: false, // discards the depth buffer after use, can be faster
                }),
                stencil_ops: None,
            }),
        });

        pass.set_bind_group(
            0,
            &ctx.resource_pools
                .bind_groups
                .get(self.bind_group_0)
                .context("get global bind group")?
                .bind_group,
            &[],
        );

        self.queued_draws
            .sort_by(|a, b| a.sorting_index.cmp(&b.sorting_index));
        for queued_draw in &self.queued_draws {
            (queued_draw.draw_func)(ctx, &mut pass).context("drawing a view")?;
        }

        Ok(())
    }

    /// Applies tonemapping and draws the final result of a `ViewBuilder` to a given output `RenderPass`.
    ///
    /// The bound surface(s) on the `RenderPass` are expected to be the same format as specified on `Context` creation.
    pub fn composite<'a>(
        &self,
        ctx: &'a RenderContext,
        pass: &mut wgpu::RenderPass<'a>,
    ) -> anyhow::Result<()> {
        pass.set_bind_group(
            0,
            &ctx.resource_pools
                .bind_groups
                .get(self.bind_group_0)
                .context("get global bind group")?
                .bind_group,
            &[],
        );

        let tonemapper = ctx
            .renderers
            .get::<Tonemapper>()
            .context("get tonemapper")?;
        tonemapper
            .draw(&ctx.resource_pools, pass, &self.tonemapping_draw_data)
            .context("perform tonemapping")
    }
}
