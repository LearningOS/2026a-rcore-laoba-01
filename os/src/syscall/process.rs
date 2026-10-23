//! Process management syscalls
use crate::task::{change_program_brk, current_memory_set, exit_current_and_run_next, get_syscall_count, suspend_current_and_run_next};
use crate::mm::{PageTable, PTEFlags, VirtAddr, MapPermission};
use crate::task::current_user_token;
use crate::timer::get_time_us;
#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    let us = get_time_us();
    let sec = us / 1_000_000;
    let usec = us % 1_000_000;
    let va = ts as usize;
    let page_table = PageTable::from_token(current_user_token());
    // `usec` is written at `va + 8`, which may fall into the next page.
    if !write_user_usize(&page_table, va, sec) || !write_user_usize(&page_table, va + 8, usec) {
        return -1;
    }
    0
}

/// Write one `usize` into the user page containing `va`.
fn write_user_usize(page_table: &PageTable, va: usize, value: usize) -> bool {
    let vpn = VirtAddr::from(va).floor();
    let off = VirtAddr::from(va).page_offset();
    let pte = match page_table.translate(vpn) {
        Some(pte) if pte.is_valid()
            && pte.flags().contains(PTEFlags::U)
            && pte.writable() => pte,
        _ => return false,
    };
    unsafe {
        let p = pte.ppn().get_bytes_array().as_mut_ptr().add(off) as *mut usize;
        *p = value;
    }
    true
}


/// TODO: Finish sys_trace to pass testcases
/// HINT: You might reimplement it with virtual memory management.
pub fn sys_trace(_trace_request: usize, _id: usize, _data: usize) -> isize {
   let token=current_user_token();
   let page_table = PageTable::from_token(token);
   let va=_id;
   let vpn = VirtAddr::from(va).floor();
   let off = VirtAddr::from(va).page_offset();
   let pte = match page_table.translate(vpn) {
         Some(pte) if pte.is_valid() && pte.flags().contains(PTEFlags::U) => pte,
          _ => return -1,
      };
   let bytes=pte.ppn().get_bytes_array();
   match _trace_request{
   0=>{
      if !pte.readable() {return -1;}
      bytes[off] as isize
    },
  1=>{
    if !pte.writable() {return -1;}
     bytes[off]=_data as u8;
     0
   },
   2=>get_syscall_count(_id) as isize,
    _=>-1,
   }
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    if !VirtAddr::from(start).aligned() { return -1; }
    if prot == 0 || prot & !0x7 != 0 { return -1; }
    if len == 0 { return 0; } 
   let start_vpn = VirtAddr::from(start).floor();
  let end_vpn   = VirtAddr::from(start + len).ceil();
    let mut perm = MapPermission::U; 
   if prot & 0b1 != 0 { perm |= MapPermission::R; }   // bit0
   if prot&0b10!= 0 {perm |= MapPermission::W;}
   if prot&0b100!= 0 {perm |= MapPermission::X;}
    let mem = current_memory_set();
      if mem.is_overlapped(start_vpn, end_vpn) {
          return -1;
      }
      mem.insert_framed_area(VirtAddr::from(start), VirtAddr::from(start + len), perm);
      0
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    if len == 0 { return -1; }
    let end = start + len;
    if !VirtAddr::from(start).aligned() { return -1; }
    if !VirtAddr::from(end).aligned() { return -1; }
    current_memory_set().unmap(VirtAddr::from(start), VirtAddr::from(end))
}
/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
