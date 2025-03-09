use std::sync::Arc;
use crate::frame::{Pixelate, PixelFormat, Frame};
#[cfg(feature = "jpeg")]
use crate::frame::{MJPG};
use crate::camera::{FrameSource};
#[allow(unused_imports)]
use crate::error::{Result, Error};

use std::marker::PhantomData;

#[cfg(feature = "jpeg")]
use turbojpeg::{Image, PixelFormat as JpegPixelFormat, Decompressor};

#[cfg(feature = "jpeg")]
pub use turbojpeg::{Compressor, Subsamp};


#[cfg(feature = "jpeg")]
#[allow(dead_code)]
pub struct JPGSource<F: PixelFormat, S: FrameSource<F>> {
    source: S,
    last_frame: usize,
    compressor: Compressor,
    _format: PhantomData<F>,
    buf: Vec<u8>,
}

#[cfg(feature = "jpeg")]
#[allow(dead_code)]
impl<F: PixelFormat, S: FrameSource<F>> JPGSource<F, S> {
    pub fn new(source: S) -> JPGSource<F, S> {
        Self::new_with_compressor(source, Compressor::new().unwrap())
    }

    pub fn new_with_compressor(source: S, compressor: Compressor) -> JPGSource<F, S> {
        JPGSource {
            source,
            last_frame: 0,
            compressor,
            buf: Vec::new(),
            _format: PhantomData,
        }
    }
}

macro_rules! mjpg_source {
    ($fmt:ty, $jpeg:expr) => {
        #[cfg(feature = "jpeg")]
        impl<S: FrameSource<$fmt>> FrameSource<MJPG> for JPGSource<$fmt, S> {
            fn get_frame(&mut self) -> Result<Option<Arc<Frame<MJPG>>>> {
                let Some(frame) = self.source.get_frame()? else {
                    return Ok(None);
                };


                let width = frame.width();
                let height = frame.height();

                if self.buf.is_empty() {
                    let len = self.compressor.buf_len(width, height).unwrap();
                    self.buf.reserve(len);
                    unsafe {
                        self.buf.set_len(len);
                    }
                }

                let image: Image<&[u8]> = Image {
                    pixels: &frame.bytes(),
                    width,
                    height,
                    pitch: frame.width() * <$fmt>::byte_count(),
                    format: $jpeg,
                };

                // FIXME this whole allocation for each frame thing ain't great, but I don't
                // feel like improving it right now.
                // let Ok(size) = self.compressor.compress_to_slice(image, &mut self.buf) else {
                //     return Err(Error::Unknown);
                // };
                let size = self.compressor.compress_to_slice(image, &mut self.buf).unwrap();

                let out = Frame::new(&self.buf[..size], width, height);
                let out = Arc::new(out);
                self.last_frame = self.source.last_frame_id();
                Ok(Some(out))
            }
            fn start(&mut self) -> Result<()> {
                self.source.start()
            }

            fn stop(&mut self) -> Result<()> {
                self.source.stop()
            }

            fn last_frame_id(&self) -> usize {
                self.last_frame
            }
        }
    }
}

mjpg_source!(crate::frame::RGB, JpegPixelFormat::RGB);
mjpg_source!(crate::frame::BGR, JpegPixelFormat::BGR);
mjpg_source!(crate::frame::RGBA, JpegPixelFormat::RGBA);
mjpg_source!(crate::frame::BGRA, JpegPixelFormat::BGRA);
mjpg_source!(crate::frame::Luma, JpegPixelFormat::GRAY);

#[allow(dead_code)]
#[cfg(feature = "jpeg")]
pub struct JPGUnpacker<F: PixelFormat, S: FrameSource<MJPG>> {
    source: S,
    last_frame: usize,
    decompressor: Decompressor,
    buf: Vec<u8>,
    _format: PhantomData<F>,
}

#[allow(dead_code)]
#[cfg(feature = "jpeg")]
impl<F: PixelFormat, S: FrameSource<MJPG>> JPGUnpacker<F, S> {
    pub fn new(source: S) -> JPGUnpacker<F, S> {
        Self::new_with_decompressor(source, Decompressor::new().unwrap())
    }

    pub fn new_with_decompressor(source: S, decompressor: Decompressor) -> JPGUnpacker<F, S> {
        JPGUnpacker {
            source,
            last_frame: 0,
            decompressor,
            buf: Vec::new(),
            _format: PhantomData,
        }
    }
}

macro_rules! mjpg_unpack {
    ($jpeg:expr, $fmt:ty) => {
        #[cfg(feature = "jpeg")]
        impl<S: FrameSource<MJPG>> FrameSource<$fmt> for JPGUnpacker<$fmt, S> {
            fn get_frame(&mut self) -> Result<Option<Arc<Frame<$fmt>>>> {
                let Some(frame) = self.source.get_frame()? else {
                    return Ok(None);
                };


                let width = frame.width();
                let height = frame.height();

                if self.buf.is_empty() {
                    self.buf.reserve(width * height);
                    unsafe{self.buf.set_len(width * height)};
                }

                let image: Image<&mut [u8]> = Image {
                    pixels: &mut self.buf,
                    width,
                    height,
                    pitch: frame.width() * <$fmt>::byte_count(),
                    format: $jpeg,
                };

                // FIXME this whole allocation for each frame thing ain't great, but I don't
                // feel like improving it right now.
                // let Ok(size) = self.compressor.compress_to_slice(image, &mut self.buf) else {
                //     return Err(Error::Unknown);
                // };
                self.decompressor.decompress(frame.bytes(), image).map_err(|_| Error::FrameData)?;

                let out = Frame::new(&*self.buf, width, height);
                let out = Arc::new(out);
                self.last_frame = self.source.last_frame_id();
                Ok(Some(out))
            }
            fn start(&mut self) -> Result<()> {
                self.source.start()
            }

            fn stop(&mut self) -> Result<()> {
                self.source.stop()
            }

            fn last_frame_id(&self) -> usize {
                self.last_frame
            }
        }
    }
}

mjpg_unpack!(JpegPixelFormat::RGB, crate::frame::RGB);
mjpg_unpack!(JpegPixelFormat::BGR, crate::frame::BGR);
mjpg_unpack!(JpegPixelFormat::RGBA, crate::frame::RGBA);
mjpg_unpack!(JpegPixelFormat::BGRA, crate::frame::BGRA);
mjpg_unpack!(JpegPixelFormat::GRAY, crate::frame::Luma);

pub enum Rotation {
    Clockwise90,
    Clockwise180,
    Clockwise270,
    /// Equivalent to Clockwise270
    Counter90,
    /// Equivalent to Clockwise180
    Counter180,
    /// Equivalent to Clockwise90
    Counter270,
}

pub struct Rotate<F: PixelFormat, S: FrameSource<F>> {
    method: Rotation,
    source: S,
    _format: PhantomData<F>,
    // I'd like to have a buffer that doesn't get reallocated every time,
    // but the Arc makes that difficult.
    // buffer: Option<Arc<Frame>>,
}

impl<F: PixelFormat, S: FrameSource<F>> Rotate<F, S> {
    pub fn new(method: Rotation, source: S) -> Rotate<F, S> {
        Rotate {
            method,
            source,
            _format: PhantomData,
            // buffer: None,
        }
    }
}

impl<F: PixelFormat, S: FrameSource<F>> FrameSource<F> for Rotate<F, S> {
    fn get_frame(&mut self) -> Result<Option<Arc<Frame<F>>>> {
        let Some(frame) = self.source.get_frame()? else {
            return Ok(None);
        };

        // if self.buffer.is_none() {
        //     let width = frame.width();
        //     let height = frame.height();
        // 
        //     let mut buf = Vec::with_capacity(width * height);
        //     unsafe {buf.set_len(width * height);}
        //     self.buffer = Some(Arc::new(Frame::new(buf, height, width)));
        // }

        // let mut buf = unsafe {
        //     self.buffer.unwrap_unchecked().make_mut()
        // };
        let rot_frame = match self.method {
            Rotation::Clockwise90 | Rotation::Counter270 => {
                frame.rotate90()
            }
            Rotation::Clockwise180 | Rotation::Counter180 => {
                frame.rotate180()
            }
            Rotation::Clockwise270 | Rotation::Counter90 => {
                frame.rotate270()
            }
        };

        Ok(Some(Arc::new(rot_frame)))
    }

    fn start(&mut self) -> Result<()> {
        self.source.start()
    }

    fn stop(&mut self) -> Result<()> {
        self.source.stop()
    }

    fn last_frame_id(&self) -> usize {
        self.source.last_frame_id()
    }
}

pub enum Reflection {
    Vertical,
    Horizontal,
}

pub struct Reflect<F: PixelFormat, S: FrameSource<F>> {
    method: Reflection,
    source: S,
    _format: PhantomData<F>,
}

impl<F: PixelFormat, S: FrameSource<F>> Reflect<F, S> {
    pub fn new(method: Reflection, source: S) -> Reflect<F, S> {
        Reflect {
            method,
            source,
            _format: PhantomData,
        }
    }
}

impl<F: PixelFormat, S: FrameSource<F>> FrameSource<F> for Reflect<F, S> {
    fn get_frame(&mut self) -> Result<Option<Arc<Frame<F>>>> {
        let Some(frame) = self.source.get_frame()? else {
            return Ok(None);
        };

        let out_frame = match self.method {
            Reflection::Vertical => {
                frame.flip_vertical()
            }
            Reflection::Horizontal => {
                frame.flip_horizontal()
            }
        };

        Ok(Some(Arc::new(out_frame)))
    }
    
    fn start(&mut self) -> Result<()> {
        self.source.start()
    }

    fn stop(&mut self) -> Result<()> {
        self.source.stop()
    }

    fn last_frame_id(&self) -> usize {
        self.source.last_frame_id()
    }
}

pub struct Convert<F: PixelFormat, T: PixelFormat, S: FrameSource<F>> {
    source: S,
    _from_format: PhantomData<F>,
    _to_format: PhantomData<T>,
}

impl<F: PixelFormat, T: PixelFormat, S: FrameSource<F>> Convert<F, T, S> {
    pub fn new(source: S) -> Convert<F, T, S> {
        Convert {
            source,
            _from_format: PhantomData,
            _to_format: PhantomData,
        }
    }
}

use crate::frame::{RGB, Luma};
impl<S: FrameSource<RGB>> FrameSource<Luma> for Convert<RGB, Luma, S> {
    fn get_frame(&mut self) -> Result<Option<Arc<Frame<Luma>>>> {
        let Some(frame) = self.source.get_frame()?  else {
            // println!("frame not got");
            return Ok(None);
        };

        let len = frame.width() * frame.height();
        let mut data = Vec::with_capacity(len);
        unsafe {data.set_len(len)};

        for (i, p) in frame.pixels().unwrap().enumerate() {
            let r = 0.299 * p[0] as f64;
            let g = 0.597 * p[1] as f64;
            let b = 0.114 * p[2] as f64;
            data[i] = (r + g + b) as u8;

        }

        let out = Frame::new(data, frame.width(), frame.height());

        Ok(Some(Arc::new(out)))
    }

    fn start(&mut self) -> Result<()> {
        self.source.start()
    }

    fn stop(&mut self) -> Result<()> {
        self.source.stop()
    }

    fn last_frame_id(&self) -> usize {
        self.source.last_frame_id()
    }
}
