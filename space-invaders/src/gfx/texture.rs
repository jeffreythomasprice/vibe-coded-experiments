pub struct Texture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

impl Texture {
    pub fn new_with_size(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());

        Self {
            texture,
            view,
            sampler,
        }
    }

    pub fn new_with_image(device: &wgpu::Device, queue: &wgpu::Queue, image: &image::DynamicImage) -> Self {
        let rgba = image.to_rgba8();
        let (width, height) = rgba.dimensions();
        Self::new_with_image_bytes(device, queue, width, height, &rgba)
    }

    pub fn new_with_image_bytes(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        bytes: &[u8],
    ) -> Self {
        let tex = Self::new_with_size(device, width, height);
        tex.update_with_image_bytes(queue, 0, 0, width, height, bytes);
        tex
    }

    pub fn width(&self) -> u32 {
        self.texture.width()
    }

    pub fn height(&self) -> u32 {
        self.texture.height()
    }

    pub fn update_with_image(&self, queue: &wgpu::Queue, x: u32, y: u32, image: &image::DynamicImage) {
        let rgba = image.to_rgba8();
        let (width, height) = rgba.dimensions();
        self.update_with_image_bytes(queue, x, y, width, height, &rgba);
    }

    pub fn update_with_image_bytes(
        &self,
        queue: &wgpu::Queue,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        bytes: &[u8],
    ) {
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            bytes,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }
}
