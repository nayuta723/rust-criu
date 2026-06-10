use std::os::unix::io::AsRawFd;
use std::os::unix::process::CommandExt;
use std::time::Duration;

fn orphan_pts_master_test(criu_bin_path: &str) {
    if unsafe { libc::geteuid() } != 0 {
        println!("Running orphan_pts_master_test: skip (not root)");
        return;
    }

    println!("Running orphan_pts_master_test");

    let master_fd = unsafe { libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY) };
    if master_fd < 0 {
        println!("Running orphan_pts_master_test: skip (posix_openpt failed)");
        return;
    }
    if unsafe { libc::grantpt(master_fd) } != 0 || unsafe { libc::unlockpt(master_fd) } != 0 {
        unsafe { libc::close(master_fd) };
        println!("Running orphan_pts_master_test: skip (grantpt/unlockpt failed)");
        return;
    }
    let mut pts_buf = vec![0u8; 64];
    let ret = unsafe {
        libc::ptsname_r(
            master_fd,
            pts_buf.as_mut_ptr() as *mut libc::c_char,
            pts_buf.len(),
        )
    };
    if ret != 0 {
        unsafe { libc::close(master_fd) };
        println!("Running orphan_pts_master_test: skip (ptsname_r failed)");
        return;
    }
    let slave_path = unsafe { std::ffi::CStr::from_ptr(pts_buf.as_ptr() as *const libc::c_char) }
        .to_string_lossy()
        .into_owned();
    let slave_fd = unsafe {
        libc::open(
            std::ffi::CString::new(slave_path).unwrap().as_ptr(),
            libc::O_RDWR,
        )
    };
    if slave_fd < 0 {
        unsafe { libc::close(master_fd) };
        println!("Running orphan_pts_master_test: skip (open slave failed)");
        return;
    }

    let mut child = unsafe {
        std::process::Command::new("test/loop_pts")
            .arg(slave_fd.to_string())
            .pre_exec(move || {
                libc::fcntl(slave_fd, libc::F_SETFD, 0);
                Ok(())
            })
            .spawn()
            .expect("failed to spawn loop_pts")
    };
    let child_pid = child.id() as libc::pid_t;

    unsafe { libc::close(slave_fd) };

    loop {
        let mut pgrp: libc::pid_t = -1;
        if unsafe { libc::ioctl(master_fd, libc::TIOCGPGRP, &mut pgrp) } == 0 && pgrp > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    let img_dir = "test/orphan_pts_images";
    if let Err(e) = std::fs::create_dir(img_dir) {
        if e.kind() != std::io::ErrorKind::AlreadyExists {
            unsafe {
                libc::kill(child_pid, libc::SIGKILL);
                libc::close(master_fd);
            }
            let _ = child.wait();
            return;
        }
    }
    let directory = std::fs::File::open(img_dir).unwrap();

    let mut criu = rust_criu::Criu::new_with_criu_path(criu_bin_path.to_string()).unwrap();
    criu.set_pid(child_pid);
    criu.set_images_dir_fd(directory.as_raw_fd());
    criu.set_log_file("dump.log".to_string());
    criu.set_log_level(4);
    criu.set_shell_job(true);

    println!("Dumping PID {}", child_pid);
    if let Err(e) = criu.dump() {
        unsafe {
            libc::kill(child_pid, libc::SIGKILL);
            libc::close(master_fd);
        }
        let _ = child.wait();
        panic!("Dumping process failed with {:#?}", e);
    }
    unsafe { libc::close(master_fd) };
    let _ = child.wait();

    println!("Restoring PID {}", child_pid);
    let directory = std::fs::File::open(img_dir).unwrap();
    let mut criu = rust_criu::Criu::new_with_criu_path(criu_bin_path.to_string()).unwrap();
    let dir_fd = directory.as_raw_fd();
    criu.set_images_dir_fd(dir_fd);
    criu.set_work_dir_fd(dir_fd);
    criu.set_log_file("restore.log".to_string());
    criu.set_log_level(4);
    criu.set_notify_scripts(true);
    criu.set_shell_job(true);
    criu.set_orphan_pts_master(true);

    if let Err(e) = criu.restore() {
        unsafe {
            libc::kill(child_pid, libc::SIGKILL);
            libc::waitpid(child_pid, std::ptr::null_mut(), 0);
        }
        panic!(
            "Restoring process failed with {:#?}\nsee {}/restore.log for details",
            e, img_dir
        );
    }

    let master_fd = criu
        .take_orphan_pts_master_fd()
        .expect("orphan-pts-master fd not received after restore");
    assert!(
        unsafe { libc::isatty(master_fd.as_raw_fd()) } != 0,
        "received fd is not a TTY (master)"
    );

    unsafe {
        libc::kill(child_pid, libc::SIGKILL);
        libc::waitpid(child_pid, std::ptr::null_mut(), 0);
    }

    println!("Cleaning up");
    let _ = std::fs::remove_dir_all(img_dir);
}

#[test]
fn test() {
    let Some(criu_bin_path) = std::env::var("CRIU_BINARY").ok() else {
        eprintln!("skip: CRIU_BINARY not set");
        return;
    };
    orphan_pts_master_test(&criu_bin_path);
}
