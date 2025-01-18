#![allow(dead_code)]

use std::marker::PhantomData;
use std::ops::{Index, IndexMut};
use std::ops::{RangeBounds, Bound};

use vistream_protocol::camera::PixelFormat as ProtoPixelFormat;

pub trait PixelFormat: Clone {
    fn byte_count() -> usize;
    fn proto_format() -> ProtoPixelFormat;
}

#[derive(Clone, Copy)]
pub struct RGB;
impl PixelFormat for RGB {
    fn byte_count() -> usize {3}
    fn proto_format() -> ProtoPixelFormat {ProtoPixelFormat::RGB}
}

#[derive(Clone, Copy)]
pub struct BGR;
impl PixelFormat for BGR {
    fn byte_count() -> usize {3}
    fn proto_format() -> ProtoPixelFormat {ProtoPixelFormat::BGR}
}

#[derive(Clone, Copy)]
pub struct RGBA;
impl PixelFormat for RGBA {
    fn byte_count() -> usize {4}
    fn proto_format() -> ProtoPixelFormat {ProtoPixelFormat::RGBA}
}

#[derive(Clone, Copy)]
pub struct BGRA;
impl PixelFormat for BGRA {
    fn byte_count() -> usize {4}
    fn proto_format() -> ProtoPixelFormat {ProtoPixelFormat::BGRA}
}

#[derive(Clone, Copy)]
pub struct YUYV;
impl PixelFormat for YUYV {
    fn byte_count() -> usize {4}
    fn proto_format() -> ProtoPixelFormat {ProtoPixelFormat::YUYV}
}

#[derive(Clone, Copy)]
pub struct MJPG;
impl PixelFormat for MJPG {
    fn byte_count() -> usize {1}
    fn proto_format() -> ProtoPixelFormat {ProtoPixelFormat::MJPEG}
}


pub struct Pixel<'a, F: PixelFormat> {
    data: *const u8,
    _lifetime: PhantomData<&'a F>,
    // source: &'a Frame<F>,
    // start: usize,
}

impl<'a, F: PixelFormat> Pixel<'a, F> {
    fn new(data: &'a [u8]) -> Pixel<'a, F> {
        Pixel {
            data: data.as_ptr(),
            _lifetime: PhantomData,
        }
    }

    fn get(&'a self, index: usize) -> Option<&'a u8> {
        unsafe {
            if index < F::byte_count() {
                Some(&*self.data.add(index))
            } else {
                None
            }
        }
    }
}

impl<'a, F: PixelFormat> Index<usize> for Pixel<'a, F> {
    type Output = u8;
    fn index(&self, index: usize) -> &u8 {
        self.get(index).unwrap()
    }
}

pub struct PixelMut<'a, F: PixelFormat> {
    data: *mut u8,
    _lifetime: PhantomData<&'a F>,
    // source: &'a mut Frame<F>,
    // start: usize,
}

impl<'a, F: PixelFormat> PixelMut<'a, F> {
    fn new(data: &'a mut [u8]) -> PixelMut<'a, F> {
        PixelMut {
            data: data.as_mut_ptr(),
            _lifetime: PhantomData,
        }
    }

    fn get(&'a self, index: usize) -> Option<&'a u8> {
        unsafe {
            if index < F::byte_count() {
                Some(&*self.data.add(index))
            } else {
                None
            }
        }
    }

    // Not specifying lifetime here fo appease IndexMut doesn't make me happy, but it's probably fine.
    fn get_mut(&mut self, index: usize) -> Option<&mut u8> {
        unsafe {
            if index < F::byte_count() {
                Some(&mut *self.data.add(index))
            } else {
                None
            }
        }
    }
}

impl<'a, F: PixelFormat> Index<usize> for PixelMut<'a, F> {
    type Output = u8;
    fn index(&self, index: usize) -> &u8 {
        self.get(index).unwrap()
    }
}


// I would really like to have this...
// That said, most mutation that would make ergonomics nice should really be happening on the GPU
impl<'a, F: PixelFormat> IndexMut<usize> for PixelMut<'a, F> {
    fn index_mut(&mut self, index: usize) -> &mut u8 {
        self.get_mut(index).unwrap()
    }
}

pub trait Pixelate<'a, F: PixelFormat> {
    fn get_pixel(&'a self, index: usize) -> Option<Pixel<'a, F>>;

    fn row_offset(&self) -> usize;
    fn col_offset(&self) -> usize;

    fn width(&self) -> usize;
    fn height(&self) -> usize;

    fn len(&self) -> usize {
        self.width() * self.height()
    }

    fn byte_len(&self) -> usize {
        self.len() * F::byte_count()
    }

    fn coord(&self, index: usize) -> Option<(usize, usize)> {
        if index < self.len() {
            Some((index % self.width(), index / self.width()))
        } else {
            None
        }
    }
}

pub trait PixelateMut<'a, F: PixelFormat>: Pixelate<'a, F> {
    fn get_pixel_mut(&'a mut self, index: usize) -> Option<PixelMut<'a, F>>;
}

pub struct PixelIter<'a, F: PixelFormat, P> where
P: Pixelate<'a, F> {
    source: &'a P,
    index: usize,
    _format: PhantomData<F>,
}

impl<'a, F: PixelFormat, P: Pixelate<'a, F>> PixelIter<'a, F, P> {
    fn new(source: &'a P) -> PixelIter<'a, F, P> {
        PixelIter {
            source,
            index: 0,
            _format: PhantomData,
        }
    }
}

impl<'a, F: PixelFormat + 'a, P: Pixelate<'a, F>> Iterator for PixelIter<'a, F, P> {
    type Item = Pixel<'a, F>;
    fn next(&mut self) -> Option<Self::Item> {
        let res = self.source.get_pixel(self.index);
        self.index += 1;
        res
    }
}

pub struct PixelIterMut<'a, F: PixelFormat, P> where
P: PixelateMut<'a, F> {
    source: *mut P,
    index: usize,
    _lifetime: PhantomData<&'a P>,
    _format: PhantomData<F>,
}

impl<'a, F: PixelFormat, P: PixelateMut<'a, F>> PixelIterMut<'a, F, P> {
    pub fn new(source: &'a mut P) -> PixelIterMut<'a, F, P> {
        PixelIterMut {
            source: source as *mut P,
            index: 0,
            _lifetime: PhantomData,
            _format: PhantomData,
        }
    }
}

impl<'a, F: PixelFormat + 'a, P: PixelateMut<'a, F>> Iterator for PixelIterMut<'a, F, P> {
    type Item = PixelMut<'a, F>;
    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            let res = (&mut *self.source).get_pixel_mut(self.index);
            self.index += 1;
            res
        }
    }
}

#[derive(Clone)]
pub struct Frame<F: PixelFormat> {
    width: usize,
    height: usize,
    data: Box<[u8]>, // possibly generalize later?
    data_valid: bool,
    _format: PhantomData<F>,
}

// FIXME This is a strong indication of code smell. Should frame be inherently thread safe?
unsafe impl<F: PixelFormat> Send for Frame<F> {}
unsafe impl<F: PixelFormat> Sync for Frame<F> {}

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("frame data is not in a pixelatable format (probably MJPG)")]
    DataFormat,
}

type FrameResult<T> = Result<T, FrameError>;

impl<'a, F: PixelFormat> Frame<F> {

    pub fn new<T: Into<Box<[u8]>>>(data: T, width: usize, height: usize) -> Frame<F> { 
        let data = data.into();
        Frame {
            width,
            height,
            data_valid: data.len() == width * height * F::byte_count(),
            data: data.into(),
            _format: PhantomData,
        }
    }

    pub fn is_pixelable(&self) -> bool {
        self.data_valid
    }

    pub fn pixels(&'a self) -> FrameResult<PixelIter<F, Self>> {
        if !self.is_pixelable() {
            Err(FrameError::DataFormat)
        } else {
            Ok(PixelIter::new(self))
        }
    }
    
    pub fn pixels_mut(&'a mut self) -> FrameResult<PixelIterMut<'a, F, Self>> {
        if !self.is_pixelable() {
            Err(FrameError::DataFormat)
        } else {
            Ok(PixelIterMut::new(self))
        }
    }

    pub fn view<R: RangeBounds<usize>, C: RangeBounds<usize>>(&'a self, rows: R, cols: C) -> FrameResult<Option<FrameView<'a, F, Self>>> {
        if !self.is_pixelable() {
            Err(FrameError::DataFormat)
        } else {
            Ok(FrameView::new(self, rows, cols))
        }
    }
    pub fn rows<R: RangeBounds<usize>>(&'a self, rows: R) -> FrameResult<Option<FrameView<'a, F, Self>>> {
        self.view(rows, ..)
    }
    pub fn cols<C: RangeBounds<usize>>(&'a self, cols: C) -> FrameResult<Option<FrameView<'a, F, Self>>> {
        self.view(.., cols)
    }

    pub fn view_mut<R: RangeBounds<usize>, C: RangeBounds<usize>>(&'a mut self, rows: R, cols: C) -> FrameResult<Option<FrameViewMut<'a, F, Self>>> {
        if !self.is_pixelable() {
            Err(FrameError::DataFormat)
        } else {
            Ok(FrameViewMut::new(self, rows, cols))
        }
    }
    pub fn rows_mut<R: RangeBounds<usize>>(&'a mut self, rows: R) -> FrameResult<Option<FrameViewMut<'a, F, Self>>> {
        self.view_mut(rows, ..)
    }
    pub fn cols_mut<C: RangeBounds<usize>>(&'a mut self, cols: C) -> FrameResult<Option<FrameViewMut<'a, F, Self>>> {
        self.view_mut(.., cols)
    }

    pub fn bytes(&self) -> &Box<[u8]> {
        &self.data
    }
}


impl<'a, F: PixelFormat> Pixelate<'a, F> for Frame<F> {
    fn get_pixel(&'a self, index: usize) -> Option<Pixel<'a, F>> {
        if index < self.len() {
            let index = index * F::byte_count();
            Some(Pixel::new(&self.data[index..index+F::byte_count()]))
        } else {
            None
        }
    }

    fn row_offset(&self) -> usize {0}
    fn col_offset(&self) -> usize {0}

    fn width(&self) -> usize {
        self.width
    }
    
    fn height(&self) -> usize {
        self.height
    }
}

impl<'a, F: PixelFormat> PixelateMut<'a, F> for Frame<F> {
    fn get_pixel_mut(&'a mut self, index: usize) -> Option<PixelMut<'a, F>> {
        if index < self.len() {
            let index = index * F::byte_count();
            Some(PixelMut::new(&mut self.data[index..index+F::byte_count()]))
        } else {
            None
        }
    }
}

pub struct FrameView<'a, F: PixelFormat, P: Pixelate<'a, F>> {
    source: &'a P,
    start_row: usize,
    end_row: usize,
    start_col: usize,
    end_col: usize,
    _format: PhantomData<F>,
}

impl<'a, F: PixelFormat, P: Pixelate<'a, F>> FrameView<'a, F, P> {

    pub fn new<R: RangeBounds<usize>, C: RangeBounds<usize>>(source: &'a P, rows: R, cols: C) -> Option<FrameView<'a, F, P>> {
        let start_row = match rows.start_bound() {
            Bound::Included(n) => *n,
            Bound::Excluded(n) => *n-1,
            Bound::Unbounded => 0,
        };

        let end_row = match rows.end_bound() {
            Bound::Included(n) => *n+1,
            Bound::Excluded(n) => *n,
            Bound::Unbounded => source.height(),
        };

        let start_col = match cols.start_bound() {
            Bound::Included(n) => *n,
            Bound::Excluded(n) => *n-1,
            Bound::Unbounded => 0,
        };

        let end_col = match cols.end_bound() {
            Bound::Included(n) => *n+1,
            Bound::Excluded(n) => *n,
            Bound::Unbounded => source.width(),
        };
        
        if end_row > source.height() || end_col > source.width() {
            return None
        }
        

        Some(FrameView {
            source,
            start_row,
            end_row,
            start_col,
            end_col,
            _format: PhantomData,
        })
    }

    pub fn pixels(&'a self) -> PixelIter<F, Self> {
        PixelIter::new(self)
    }
    
    fn view<R: RangeBounds<usize>, C: RangeBounds<usize>>(&'a self, rows: R, cols: C) -> Option<FrameView<'a, F, Self>> {
        FrameView::new(self, rows, cols)
    }
    fn rows<R: RangeBounds<usize>>(&'a self, rows: R) -> Option<FrameView<'a, F, Self>> {
        self.view(rows, ..)
    }
    fn cols<C: RangeBounds<usize>>(&'a self, cols: C) -> Option<FrameView<'a, F, Self>> {
        self.view(.., cols)
    }

    fn resolve_index(&self, index: usize) -> usize {
        let w = self.width();
        let col_offset = index % w;
        let row_offset = index / w;

        let index = (self.start_row + row_offset) * self.source.width() + self.start_col + col_offset;
        index
    }
}

impl<'a, F: PixelFormat, P: Pixelate<'a, F>> Pixelate<'a, F> for FrameView<'a, F, P> {
    fn get_pixel(&'a self, index: usize) -> Option<Pixel<'a, F>> {
        if index < self.len() {
            let index = self.resolve_index(index);
            self.source.get_pixel(index)
        } else {
            None
        }
    }

    fn row_offset(&self) -> usize {
        self.start_row
    }

    fn col_offset(&self) -> usize {
        self.start_col
    }

    fn width(&self) -> usize {
        self.end_col - self.start_col
    }

    fn height(&self) -> usize {
        self.end_row - self.start_row
    }
}

pub struct FrameViewMut<'a, F: PixelFormat, P: Pixelate<'a, F>> {
    source: &'a mut P,
    start_row: usize,
    end_row: usize,
    start_col: usize,
    end_col: usize,
    _format: PhantomData<F>,
}

impl<'a, F: PixelFormat, P: PixelateMut<'a, F>> FrameViewMut<'a, F, P> {

    pub fn new<R: RangeBounds<usize>, C: RangeBounds<usize>>(source: &'a mut P, rows: R, cols: C) -> Option<FrameViewMut<'a, F, P>> {
        let start_row = match rows.start_bound() {
            Bound::Included(n) => *n,
            Bound::Excluded(n) => *n-1,
            Bound::Unbounded => 0,
        };

        let end_row = match rows.end_bound() {
            Bound::Included(n) => *n+1,
            Bound::Excluded(n) => *n,
            Bound::Unbounded => source.height(),
        };

        let start_col = match cols.start_bound() {
            Bound::Included(n) => *n,
            Bound::Excluded(n) => *n-1,
            Bound::Unbounded => 0,
        };

        let end_col = match cols.end_bound() {
            Bound::Included(n) => *n+1,
            Bound::Excluded(n) => *n,
            Bound::Unbounded => source.width(),
        };
        
        if end_row > source.height() || end_col > source.width() {
            return None
        }
        

        Some(FrameViewMut {
            source,
            start_row,
            end_row,
            start_col,
            end_col,
            _format: PhantomData,
        })
    }

    pub fn pixels(&'a self) -> PixelIter<F, Self> {
        PixelIter::new(self)
    }
    
    pub fn pixels_mut(&'a mut self) -> PixelIterMut<'a, F, Self> {
        PixelIterMut::new(self)
    }
    
    pub fn view<R: RangeBounds<usize>, C: RangeBounds<usize>>(&'a self, rows: R, cols: C) -> Option<FrameView<'a, F, Self>> {
        FrameView::new(self, rows, cols)
    }
    pub fn rows<R: RangeBounds<usize>>(&'a self, rows: R) -> Option<FrameView<'a, F, Self>> {
        self.view(rows, ..)
    }
    pub fn cols<C: RangeBounds<usize>>(&'a self, cols: C) -> Option<FrameView<'a, F, Self>> {
        self.view(.., cols)
    }

    pub fn view_mut<R: RangeBounds<usize>, C: RangeBounds<usize>>(&'a mut self, rows: R, cols: C) -> Option<FrameViewMut<'a, F, Self>> {
        FrameViewMut::new(self, rows, cols)
    }
    pub fn rows_mut<R: RangeBounds<usize>>(&'a mut self, rows: R) -> Option<FrameViewMut<'a, F, Self>> {
        self.view_mut(rows, ..)
    }
    pub fn cols_mut<C: RangeBounds<usize>>(&'a mut self, cols: C) -> Option<FrameViewMut<'a, F, Self>> {
        self.view_mut(.., cols)
    }
    
    
    pub fn resolve_index(&self, index: usize) -> usize {
        let w = self.width();
        let col_offset = index % w;
        let row_offset = index / w;

        let index = (self.start_row + row_offset) * self.source.width() + self.start_col + col_offset;
        index
    }
}

impl<'a, F: PixelFormat, P: PixelateMut<'a, F>> Pixelate<'a, F> for FrameViewMut<'a, F, P> {
    fn get_pixel(&'a self, index: usize) -> Option<Pixel<'a, F>> {
        if index < self.len() {
            let index = self.resolve_index(index);
            self.source.get_pixel(index)
        } else {
            None
        }
    }

    fn row_offset(&self) -> usize {
        self.start_row
    }

    fn col_offset(&self) -> usize {
        self.start_col
    }

    fn width(&self) -> usize {
        self.end_col - self.start_col
    }

    fn height(&self) -> usize {
        self.end_row - self.start_row
    }
}

impl<'a, F: PixelFormat, P: PixelateMut<'a, F>> PixelateMut<'a, F> for FrameViewMut<'a, F, P> {
    fn get_pixel_mut(&'a mut self, index: usize) -> Option<PixelMut<'a, F>> {
        if index < self.len() {
            let index = self.resolve_index(index);
            self.source.get_pixel_mut(index)
        } else {
            None
        }
    }
}

