use std::marker::PhantomData;

use bytemuck::Pod;
use wgpu::BufferUsages;

pub struct Buffer<V: Pod> {
    pub buffer: wgpu::Buffer,
    size: usize,
    stride: usize,
    _phantom: PhantomData<V>,
}

impl<V: Pod> Buffer<V> {
    pub fn new(device: &wgpu::Device, size: usize, usage: BufferUsages) -> Self {
        let stride = Self::element_size_in_bytes();
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (size * stride) as wgpu::BufferAddress,
            usage,
            mapped_at_creation: false,
        });

        Self { buffer, size, stride, _phantom: PhantomData }
    }

    pub fn new_aligned(
        device: &wgpu::Device,
        capacity: usize,
        alignment: usize,
        usage: BufferUsages,
    ) -> Self {
        let element_size = Self::element_size_in_bytes();
        let stride = ((element_size + alignment - 1) / alignment) * alignment;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (capacity * stride) as wgpu::BufferAddress,
            usage,
            mapped_at_creation: false,
        });

        Self { buffer, size: capacity, stride, _phantom: PhantomData }
    }

    pub fn write(&self, queue: &wgpu::Queue, offset: usize, data: &[V]) {
        let byte_offset = (offset * self.stride) as wgpu::BufferAddress;
        queue.write_buffer(&self.buffer, byte_offset, bytemuck::cast_slice(data));
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn stride(&self) -> usize {
        self.stride
    }

    pub fn element_size_in_bytes() -> usize {
        std::mem::size_of::<V>()
    }
}
