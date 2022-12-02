//use std::process;
use alloc::vec::Vec;
use crate::sync::{Condvar, Mutex, MutexBlocking, MutexSpin, Semaphore};
use crate::task::{block_current_and_run_next, current_process, current_task};
use crate::timer::{add_timer, get_time_ms};
use alloc::sync::Arc;

use super::thread::sys_gettid;

pub fn sys_sleep(ms: usize) -> isize {
    let expire_ms = get_time_ms() + ms;
    let task = current_task().unwrap();
    add_timer(expire_ms, task);
    block_current_and_run_next();
    0
}

// LAB5 HINT: you might need to maintain data structures used for deadlock detection
// during sys_mutex_* and sys_semaphore_* syscalls
pub fn sys_mutex_create(blocking: bool) -> isize {
    let process = current_process();
    let mutex: Option<Arc<dyn Mutex>> = if !blocking {
        Some(Arc::new(MutexSpin::new()))
    } else {
        Some(Arc::new(MutexBlocking::new()))
    };
    let mut process_inner = process.inner_exclusive_access();
    if let Some(id) = process_inner
        .mutex_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.mutex_list[id] = mutex;
        process_inner.mutex_alloc[id] = None;
        id as isize
    } else {
        process_inner.mutex_list.push(mutex);
        process_inner.mutex_alloc.push(None);
        process_inner.mutex_list.len() as isize - 1
    }
}

// LAB5 HINT: Return -0xDEAD if deadlock is detected
pub fn sys_mutex_lock(mutex_id: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    if process_inner.deadlock_detect==true{
        let current_thread = process_inner.get_current_thread();
        process_inner.mutex_request[current_thread] = Some(mutex_id);
        //let available = Vec::new();
        let thread_len = process_inner.tasks.len();
        let mutex_len = process_inner.mutex_list.len();
        let mut copy = process_inner.mutex_alloc.clone();
        let mut finish = Vec::new();
        for i in 0..thread_len{
            finish.push(false);
        }
        for j in 0..thread_len{
            for i in 0..thread_len{
                if finish[i] == true{
                    continue;
                }
                let id = process_inner.mutex_request[i];
                if let Some(id) = id{
                    if copy[id] == None{
                        finish[i] = true;
                        for k in 0..mutex_len{
                            if copy[k] == Some(id){
                                copy[k] = None;
                            }
                        }
                    }  
                }
            }
        }
        
        for i in 0..thread_len{
            if finish[i]==false{
                return -0xDEAD;
            }
        }
        
    }
    process_inner.mutex_alloc[mutex_id] = Some(process_inner.get_current_thread());
    drop(process_inner);
    drop(process);
    mutex.lock();
    0
}

pub fn sys_mutex_unlock(mutex_id: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    process_inner.mutex_alloc[mutex_id] = None;
    drop(process_inner);
    drop(process);
    mutex.unlock();
    0
}

pub fn sys_semaphore_create(res_count: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .semaphore_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.semaphore_list[id] = Some(Arc::new(Semaphore::new(res_count)));
        process_inner.sem_avil[id] = res_count;
        for i in 0..process_inner.sem_alloc.len(){
            process_inner.sem_alloc[i][id] = 0;
        }
        id
    } else {
        process_inner
            .semaphore_list
            .push(Some(Arc::new(Semaphore::new(res_count))));
        process_inner.sem_avil.push(res_count);     //lab5 initialize the variable
        //process_inner.sem_alloc.push(0 as usize);
        for i in 0..process_inner.sem_alloc.len(){
            process_inner.sem_alloc[i].push(0);
        }
        process_inner.semaphore_list.len() - 1
    };
    id as isize
}

pub fn sys_semaphore_up(sem_id: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    process_inner.sem_avil[sem_id] +=1;
    let current_thread = sys_gettid() as usize;
    process_inner.sem_alloc[current_thread][sem_id] -=1;
    
    drop(process_inner);
    sem.up();
    0
}

// LAB5 HINT: Return -0xDEAD if deadlock is detected
pub fn sys_semaphore_down(sem_id: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    let current_thread = sys_gettid() as usize;
    process_inner.sem_request[current_thread] = Some(sem_id);
    if process_inner.deadlock_detect==true{
        let thread_len = process_inner.sem_request.len();
        let sem_len = process_inner.semaphore_list.len();
        let mut work = process_inner.sem_avil.clone();
        let mut finish = Vec::new();
        for i in 0..thread_len{
            finish.push(false);
        }
        for i in 0..thread_len{       //key 
            if process_inner.sem_alloc[i].is_empty(){
                finish[i] = true;
            }
        }
        for i in 0..thread_len{
            for j in 0..thread_len{
                if finish[j]==true{
                    continue;
                }
                if let Some(id)= process_inner.sem_request[j]{
                    if work[id] >0{
                        finish[j] = true;
                        for k in 0..sem_len{
                            work[k] += process_inner.sem_alloc[j][k];
                        }
                        //break;
                    }
                }else{
                    finish[j] = true;
                    for k in 0..sem_len{
                        work[k] += process_inner.sem_alloc[j][k];
                    }
                    //break;
                }
            }
        }
        for i in 0..thread_len{
            if finish[i]==false{
                println!("The false thread is {}  --------------",i);
                return -0xDEAD;
            }
        }
    }
    
    drop(process_inner);
    sem.down();
    let mut process_inner = process.inner_exclusive_access();
    process_inner.sem_alloc[current_thread][sem_id] +=1;
    process_inner.sem_avil[sem_id] -=1;
    process_inner.sem_request[current_thread] = None;
    0
}

pub fn sys_condvar_create(_arg: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .condvar_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.condvar_list[id] = Some(Arc::new(Condvar::new()));
        id
    } else {
        process_inner
            .condvar_list
            .push(Some(Arc::new(Condvar::new())));
        process_inner.condvar_list.len() - 1
    };
    id as isize
}

pub fn sys_condvar_signal(condvar_id: usize) -> isize {
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    drop(process_inner);
    condvar.signal();
    0
}

pub fn sys_condvar_wait(condvar_id: usize, mutex_id: usize) -> isize {
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    condvar.wait(mutex);
    0
}

// LAB5 YOUR JOB: Implement deadlock detection, but might not all in this syscall
pub fn sys_enable_deadlock_detect(_enabled: usize) -> isize {
    if _enabled==0||_enabled==1{
        if _enabled==1{
            let process = current_process();
            let mut process_inner = process.inner_exclusive_access();
            process_inner.deadlock_detect = true;
        }
        return 0;
    }
    -1
}
