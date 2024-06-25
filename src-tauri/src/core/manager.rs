use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex};
use tokio::select;
use crate::core::Task;
use tokio::sync::broadcast;
use tracing::{error, info};
use crate::core::Scheduler;
use subprocess::{Exec, Popen, PopenConfig};
use anyhow::Result;

pub struct Manager {
    pub name: String,
    pub tasks: Arc<Mutex<HashMap<String, Task>>>,
    pub children: Arc<Mutex<HashMap<String, Arc<Mutex<Popen>>>>>,
    pub closer: broadcast::Receiver<String>,
}

impl Manager {
    pub fn new(closer: broadcast::Receiver<String>) -> Self {
        let name = "WindowsCore".to_string();
        let tasks = Arc::new(Mutex::new(HashMap::new()));
        let children = Arc::new(Mutex::new(HashMap::new()));
        Manager {
            name,
            tasks,
            closer,
            children,
        }
    }
}

impl Scheduler for Manager {
    async fn start(&mut self, task: Task) -> Result<()> {
        self.tasks.lock().await.insert(task.id.clone(), task.clone());

        let config = PopenConfig {
            stdout: subprocess::Redirection::Pipe,
            stderr: subprocess::Redirection::Pipe,
            ..Default::default()
        };



        info!("build task {} with id {}", task.name, task.id);
        // #[cfg(target_os = "macos")]
        let args: &[&str] = &[
            &task.command,
            "-f", "avfoundation",
            "-i", "1",
            "-r", "30",
            "-s", "1920x1080",
            "-vcodec", "libx264",
            "-preset", "ultrafast",
            "-crf", "18",
            "-pix_fmt", "yuv420p",
            "output.mp4",
            "-y"
        ];
        // full argument list for windows
        // ffmpeg -f gdigrab -framerate 30 -i desktop -vcodec libx264 -preset ultrafast -crf 18 -pix_fmt yuv420p -f mp4 t.mp4 -y
        #[cfg(target_os = "windows")]
        let args = &[
            &task.command,
            "-f", "gdigrab", "-framerate", "10", "-i", "desktop",
            "-c:v", "libx264", "-crf", "36", "-preset", "veryfast",
            "-an", "t.mp4"
        ];

        let exec = Popen::create(args, config)?;

        let child = Arc::new(Mutex::new(exec));
        self.children.lock().await.insert(task.id.clone(), Arc::clone(&child));
        let children = Arc::clone(&self.children);
        info!("Started task {} with id {}", task.name, task.id);
        tokio::spawn(async move {
            let mut c = child.lock().await;
            info!("Task is running waiting for it to stop");
            match c.wait() {
                Ok(exit_code) => {
                    info!("Task {} with id {} has stopped", task.name, task.id);
                    children.lock().await.remove(&task.id).unwrap();
                }
                Err(_) => {
                    error!("Failed to start task {}", task.name);
                }
            }
        });
        Ok(())
    }


    async fn monitor(&mut self) {
        info!("Starting monitor");
        let mut closer = self.closer.resubscribe();
        loop {
            let timer = tokio::time::sleep(tokio::time::Duration::from_secs(2));
            let mut tasks_to_restart = Vec::new();
            {
                let tasks = self.tasks.lock().await;
                let children = self.children.lock().await;
                for (k, v) in tasks.iter() {
                    if !children.contains_key(k) {
                        tasks_to_restart.push(v.clone());
                    }
                }
            }
            for task in tasks_to_restart {
                info!("Restarting task {} with id {}", task.name, task.id);
                let _ = self.start(task).await;
            }
            select! {
                _ = closer.recv() => {
                    break;
                }
                _ = timer => {}
            }
        }
        error!("Monitor stopped")
    }

    async fn restart(&mut self, task: Task) -> Result<()> {
        self.stop(task.clone()).await;
        self.start(task).await
    }

    async fn stop(&self, task: Task) {
        if let Some(clild) = self.children.lock().await.get(&task.id) {
            let mut child = clild.lock().await;
            match child.kill() {
                Ok(_) => {
                    error!("Stopped task {} with id {}", task.name, task.id);
                }
                Err(e) => {
                    error!("Failed to stop task {}: {}", task.name, e);
                }
            }
        }
    }
}