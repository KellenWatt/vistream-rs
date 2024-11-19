use std::time::Duration;

use libcamera::{
    camera::{self, CameraConfigurationStatus},
    camera_manager,
    framebuffer::AsFrameBuffer,
    framebuffer_allocator::{FrameBuffer, FrameBufferAllocator},
    framebuffer_map::MemoryMappedFrameBuffer,
    pixel_format::PixelFormat,
    properties,
    stream::StreamRole,
    request::Request,
    geometry::Size,
};



const PIXEL_FORMAT_RGB8: PixelFormat = PixelFormat::new(u32::from_le_bytes([b'R', b'G', b'2', b'4']), 0);
// const PIXEL_FORMAT_RGB888: PixelFormat = PixelFormat::RGB888;

pub trait FrameSource {
    fn get_frame(&self) -> ndarray::Array2<[u8; 3]>;
    fn frame_size(&self) -> (usize, usize);
    fn frame_id(&self) -> u64;
    // fn field_of_view(&self) -> (f32, f32);
}

pub struct Camera<'a> {
    _camref: &'a camera::Camera<'a>,
    cam: camera::ActiveCamera<'a>,
    reqs: Vec<Request>,

    frame_size: (usize, usize),
    frame_id: u64,
}

impl<'a> Camera<'a> {

    fn from_config(srcam: &'a camera::Camera<'a>,
                   // mut cam: camera::ActiveCamera<'a>,
                   mut cfgs: camera::CameraConfiguration) -> Option<Camera<'a>> {
        let mut cam = srcam.acquire().ok()?;
        match cfgs.validate() {
            CameraConfigurationStatus::Valid => Some(()),
            CameraConfigurationStatus::Adjusted => {
                if cfgs.get(0).unwrap().get_pixel_format() != PIXEL_FORMAT_RGB8 {
                    None
                } else {
                    Some(())
                }
            }
            CameraConfigurationStatus::Invalid => None,
        }?;

        cam.configure(&mut cfgs).ok()?;

        let mut alloc = FrameBufferAllocator::new(&cam);

        let cfg = cfgs.get(0).unwrap();
        let stream = cfg.stream().unwrap();
        let buffers = alloc.alloc(&stream).unwrap();

        let buffers = buffers.into_iter()
                             .map(|buf| MemoryMappedFrameBuffer::new(buf).unwrap()).collect::<Vec<_>>();

        let mut reqs = buffers.into_iter()
            .map(|buf| {
                let mut req = cam.create_request(None).unwrap();
                req.add_buffer(&stream, buf).unwrap();
                req
            }).collect::<Vec<_>>();

        Some(Camera {
            _camref: srcam,
            cam,
            reqs,
            frame_size: (cfg.get_size().width as usize, cfg.get_size().height as usize),
            frame_id: 0,
        })
    }

    pub fn new(cam: &'a camera::Camera<'a>) -> Option<Camera<'a>> {
        let mut cfgs = cam.generate_configuration(&[StreamRole::VideoRecording]).unwrap();
        cfgs.get_mut(0).unwrap().set_pixel_format(PIXEL_FORMAT_RGB8);
        cfgs.get_mut(0).unwrap().set_buffer_count(1);
        Camera::from_config(cam, cfgs)
    }

    pub fn with_size(cam: &'a camera::Camera<'a>, width: usize, height: usize) -> Option<Camera<'a>> {
        let mut cfgs = cam.generate_configuration(&[StreamRole::VideoRecording]).unwrap();
        let mut cfg = cfgs.get_mut(0).unwrap();
        cfg.set_pixel_format(PIXEL_FORMAT_RGB8);
        cfg.set_buffer_count(1);
        cfg.set_size(Size{width: width as u32, height: height as u32});
        Camera::from_config(cam, cfgs)
    }
}

impl<'a> FrameSource for Camera<'a> {
    fn get_frame(&self) -> ndarray::Array2<[u8; 3]> {
        ndarray::arr2::<_, 0>(&[])
    }

    fn frame_size(&self) -> (usize, usize) {
        self.frame_size
    }

    fn frame_id(&self) -> u64 {
        self.frame_id
    }
}
