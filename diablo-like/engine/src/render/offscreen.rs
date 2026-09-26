/// The world pass renders here instead of directly to the swapchain, at
/// `render_scale` times the swapchain's resolution. The composite pass then
/// downsamples it back into the swapchain with a single bilinear tap — a
/// standard cheap approximation of supersampling, not a true box filter.
pub struct Offscreen {
    pub width: u32,
    pub height: u32,
    pub format: wgpu::TextureFormat,
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}

impl Offscreen {
    pub fn new(
        device: &wgpu::Device,
        composite_bind_group_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen color target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("offscreen sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("offscreen bind group"),
            layout: composite_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        Self {
            width,
            height,
            format,
            view,
            bind_group,
        }
    }

    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    /// Computes the render-scaled offscreen size for a swapchain of
    /// `surface_{width,height}`, clamped to what this adapter can actually
    /// allocate (see `Capabilities::max_texture_dimension_2d`).
    pub fn scaled_size(surface_width: u32, surface_height: u32, scale: f32, max_dimension: u32) -> (u32, u32) {
        let w = ((surface_width as f32 * scale).round() as u32)
            .max(1)
            .min(max_dimension);
        let h = ((surface_height as f32 * scale).round() as u32)
            .max(1)
            .min(max_dimension);
        (w, h)
    }
}
