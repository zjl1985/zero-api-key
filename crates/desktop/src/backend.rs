use anyhow::{Result, anyhow};
use async_channel::Receiver;
use std::sync::mpsc;
use zero_api_key_lib::service::Service;

type Job = Box<dyn FnOnce(&mut Result<Service>) + Send>;

/// A single worker serializes vault mutations and keeps blocking HTTP, PBKDF2,
/// Keychain and LocalAuthentication entirely off GPUI's foreground executor.
#[derive(Clone)]
pub struct Backend(mpsc::Sender<Job>);

impl Backend {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("vault-worker".into())
            .spawn(move || {
                let mut service = Service::new();
                while let Ok(job) = receiver.recv() {
                    job(&mut service);
                }
            })
            .expect("cannot start vault worker");
        Self(sender)
    }

    pub fn execute<T: Send + 'static>(
        &self,
        job: impl FnOnce(&mut Service) -> Result<T> + Send + 'static,
    ) -> Receiver<Result<T>> {
        let (sender, receiver) = async_channel::bounded(1);
        let task = Box::new(move |service: &mut Result<Service>| {
            let result = match service {
                Ok(service) => job(service),
                Err(error) => Err(anyhow!("{error:#}")),
            };
            let _ = sender.try_send(result);
        });
        let _ = self.0.send(task);
        receiver
    }
}
