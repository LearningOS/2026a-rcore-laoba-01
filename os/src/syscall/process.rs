//! Process management syscalls
use crate::task::{change_program_brk, exit_current_and_run_next, suspend_current_and_run_next};
use crate::mm::{PageTable, PTEFlags, VirtAddr};
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
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
   let us: usize=get_time_us();
   let sec: usize=us/1_000_000; 
   let usec: usize=us%1_000_000;
   let token=current_user_token();
   let page_table=PageTable::from_token(token);
    let va = _ts as usize;
    let vpn = VirtAddr::from(va).floor();
    let off = VirtAddr::from(va).page_offset();
    let pte = match page_table.translate(vpn) {
      Some(pte) => pte,      // 盒子里有 → 拿出来
      None => return -1,     // 盒子是空的 → 野指针，直接失败
  };
  let ppn = pte.ppn();  
    let array=ppn.get_bytes_array();
     unsafe {
     let p1=array.as_mut_ptr()
                .add(off) as *mut usize;
         *p1=sec;
     }
   unsafe{
     let p2=array.as_mut_ptr()
                .add(off+8) as *mut usize;
        *p2=usec;
   }
      0
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
      bytes[off] as isize
    },
  1=>{
    if !pte.writable() {return -1;}
     bytes[off]=_data as u8;
     0
   },
    _=>-1,
   }   
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    if !VirtAddr::from(start).aligned() { return -1; }
    if prot == 0 || prot & !0x7 != 0 { return -1; }
    if len == 0 { return 0; } 
    let start_vpn=start >> 12;
    let end_vpn=(start + len) >> 12;


     0
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
  -1
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
