# Lab4 实验报告

> 姓名：徐华宇　学号：3248921201　GitHub：laoba-01

## 一、功能总结

在 ch6 的 easy-fs 上实现了 `fstat`、`linkat`、`unlinkat`：`DiskInode` 新增 `nlink` 硬链接计数，`Stat` 经 `File` trait 的 `metadata` 方法从 inode 实时读取并写回用户态；`linkat` 追加指向同一 inode 的目录项并增加 `nlink`；`unlinkat` 删除目录项、减少 `nlink`，归零时回收数据块与 inode。目录项删除采用“末项填洞 + 缩短 `size`”的 O(1) 做法，easy-fs 的磁盘布局同步升级。

验证结果：

- `ch6_file0`：基本 open/write/read/close 通过。
- `ch6_file1`：输出 `Test fstat OK!`。
- `ch6_file2`：`nlink` 依次为 2、4、3、1，输出 `Test link OK!`。
- `ch6_file3`：10 轮 open/write/close/unlink/reopen，输出 `Test mass open/unlink OK!`。
- `ch6b_filetest_simple`：输出 `file_test passed!`。
- 官方 checker（`make -C ci-user test CHAPTER=6`）：`Test passed: 31/31`，`reports/lab1.md` 至 `lab4.md` 均被识别。

## 二、关键设计与取舍

### 1. 在 `DiskInode` 中加入 `nlink`

`nlink` 是跨 `open`/`close`、跨进程的持久元数据，必须落盘，不能只在内存里缓存。加入一个 `u32` 后 `DiskInode` 从 124 字节变成 128 字节，`512 / 128 = 4`，仍然每个块放 4 个 inode。为了保持“每个块恰好 4 个 inode”且不改变原有布局的整齐性，把 `INODE_DIRECT_COUNT` 从 28 调到 27；同时把文件系统 magic 从 `0x3b800001` 升到 `0x3b800002`，让旧镜像在 `is_valid()` 处直接失败，而不是按新布局误读。`initialize()` 把 `nlink` 置为 1。

### 2. `fstat` 的数据通路

`fd_table` 的元素类型是 `Arc<dyn File + Send + Sync>`，内核拿不到具体的 `OSInode`，也无法做向下转型。因此给 `File` trait 增加一个默认返回 `None` 的 `metadata() -> Option<Stat>` 方法，由 `OSInode` 覆写；`Stdin`/`Stdout` 自然沿用默认实现。

`OSInode::metadata` 每次都从磁盘 inode 读取 `nlink`，不做缓存，否则 `link`/`unlink` 之后 `fstat` 会读到旧值。`ino` 用 `inode_id()`，`mode` 由 `is_dir()` 决定，`dev` 按实验约定写死 0。`fstat` 对 `WRONLY` 打开的 fd 也必须成功，所以不能像 `read`/`write` 那样检查 `readable()`/`writable()`。

写回用户态使用 `translated_refmut(token, st)`：先用当前进程页表把用户虚拟地址翻译成内核可写的引用，再整体赋值。`token` 必须在获取任务锁之前取，因为 `current_user_token()` 自身会再次访问当前任务。

### 3. `linkat`：追加目录项，而不是复制数据

`linkat` 先用旧名字在根目录中找到目标 inode，检查新名字不存在，然后在目录末尾追加一个 32 字节的 `DirEntry`，其中的 `inode_id` 指向**已有的 inode**，最后把该 inode 的 `nlink` 加一。文件大小、数据块都没有复制，两个名字共享同一份内容。

当前实现在 `find` 和 `link` 之间释放过一次文件系统锁；单核实验环境下不会出问题，但严格实现应把“查找旧名字”和“追加目录项”放进同一个临界区，避免中途被其他进程删除。

### 4. `unlinkat`：O(1) 删除目录项，归零才回收

目录文件的内容是连续的 `DirEntry` 数组，删除中间一项不能让 `0..file_count` 出现空洞，否则 `find` 会读到无效槽位。做法是把**最后一个**目录项搬到被删位置，再把目录 `size` 减 32 字节；这样删除是 O(1)，代价是目录项顺序会被打乱，但查找只按名字，顺序无影响。

目标 inode 的 `nlink` 通过 `fs.get_disk_inode_pos()` 定位后直接修改块缓存。这里不能调用 `Inode::clear()`：`clear()` 会再次获取文件系统锁，而 `unlink` 已经持有该锁，会直接死锁。

`nlink` 减到 0 时，调用 `clear_size()` 得到所有数据块（包括 indirect1/indirect2 指向的块），逐个 `dealloc_data()`，再 `dealloc_inode()` 清掉 inode 位图。测例注释允许不回收，但完整语义应当回收。

局限：本实现没有处理“文件仍被打开时被 unlink”的延迟回收。正式 Unix 会在最后一个 fd 关闭时才释放 inode 和数据；本实验的测例不覆盖这一场景。

### 5. 与前几章的兼容

ch6 分支保留了 ch4 的 `get_time`/`mmap`/`munmap` 移植和 ch5 的 `spawn`，因此 ch2–ch5 的测例仍然可以运行。ch6 的官方测例集合不包含 `set_priority` 与 stride 专项测例，所以这两项保持模板状态。

## 三、问答作业

### Q1 在我们的 easy-fs 中，root inode 起着什么作用？如果 root inode 中的内容损坏了，会发生什么？

**root inode 的作用**

root inode 是 inode 编号 0，是文件系统的根目录和唯一入口。`EasyFileSystem::create` 在格式化时先分配 inode 0 并把它初始化为 `Directory`；`EasyFileSystem::root_inode()` 用 inode 0 的位置构造 `Inode` 对象；内核里的 `ROOT_INODE` 静态变量在第一次访问时打开块设备并缓存它。

本实验的 easy-fs 只有一层目录，所有 `find`/`create`/`link`/`unlink` 都在 root inode 的目录项数组里按名字线性查找。没有它，文件名无法映射到 inode，任何路径解析都无从开始——内核启动时 `open_file("ch6b_initproc")` 会失败，`INITPROC` 的 `unwrap` 直接 panic。

**内容损坏的后果**

- `type_` 被改成 `File`：`find_inode_id` 里的 `assert!(disk_inode.is_dir())` 会触发 panic。
- `size` 变为 0 或过小：目录看起来为空，所有程序都找不到，系统无法启动用户态。
- `size` 过大或数据块指针损坏：`read_at` 会读到错误块甚至 inode 区之外的块，目录项被解析成垃圾，得到的 `inode_id` 可能指向任意位置；后续读 inode 会读到错误数据，或触发块缓存的断言 panic。
- `nlink`、`direct`、`indirect` 被破坏：后续 `create`/`link` 写目录项时可能覆盖其他 inode 或数据块，损坏范围会扩大。

easy-fs 只在超级块上校验 magic，没有校验和、冗余副本或 fsck，因此 root inode 损坏后通常无法自愈，只能从主机重新生成 `fs.img`（`easy-fs-fuse`）或手工修复对应块。root inode 本身没有被任何目录项引用，`unlink` 只删除 root 目录里的名字，所以正常操作不会误删它。

## 四、荣誉准则

1. 在完成本次实验的过程（含此前学习的过程）中，我曾分别与 **以下各位** 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：

    与 Claude（AI 编程助手）就 easy-fs 的目录项操作、硬链接语义和报告撰写进行过交流。

2. 此外，我也参考了 **以下资料** ，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：

    rCore-Tutorial-Book-v3、rCore-Tutorial-Guide、Linux man-pages 中 `linkat`/`unlinkat`/`fstat` 的接口说明。

3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。
我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。
我未曾也不会向他人（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。
我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。
我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按“-100”分计。

## 五、可选反馈

本章把文件描述符、inode、目录项和数据块串成了一条完整的链路，`fstat` 只读元数据，`link`/`unlink` 只改目录项和链接数，真正删除数据发生在 `nlink` 归零时——这条规则想清楚之后实现并不复杂。建议测例中补充“文件仍被打开时 unlink”的场景，这样能更完整地检验 inode 生命周期的设计。
