//! Acelerador: driver atras de interface de capacidade.
//!
//! Esta e' a decisao arquitetural que mantem futuros distintos vivos sem
//! reescrita (secao 3.5). O nucleo nunca sabe se fala com CPU, simulador
//! ou GPU. "O driver mora fora" -- principio 6.

use crate::session::Refused;

/// Modelo de custo. E' o que torna admission control possivel:
/// diferente de carga generica, o custo por token e' previsivel.
#[derive(Clone, Copy, Debug)]
pub struct CostModel {
    pub ns_per_token: u64,
    pub kv_bytes_per_token: u64,
    /// Vazao agregada que este dispositivo sustenta.
    pub max_tokens_per_sec: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct Caps {
    pub preemptible: bool,
    pub cancellable: bool,
    pub max_concurrent_sessions: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct Handle(pub u64);

pub struct Batch {
    pub session: crate::session::SessionId,
    pub tokens: u32,
}

pub trait Accelerator {
    fn cost_model(&self) -> CostModel;
    fn capabilities(&self) -> Caps;

    /// I1: submissao carrega prazo. Estourar e' retorno normal, nao excecao.
    fn submit(&mut self, work: Batch, deadline_ns: u64) -> Result<Handle, Refused>;

    fn cancel(&mut self, h: Handle);
}

/// Driver simulado: modelo de custo puro, sem hardware.
///
/// Nao e' atalho -- e' o que torna escalonador, admission control e gerencia
/// de KV deterministicos e testaveis antes de existir hardware (decisao D5).
pub struct Simulated {
    cost: CostModel,
    next_handle: u64,
    in_flight: u32,
    caps: Caps,
}

impl Simulated {
    pub const fn new(ns_per_token: u64, kv_bytes_per_token: u64, max_tps: u32) -> Self {
        Self {
            cost: CostModel { ns_per_token, kv_bytes_per_token, max_tokens_per_sec: max_tps },
            next_handle: 1,
            in_flight: 0,
            caps: Caps { preemptible: true, cancellable: true, max_concurrent_sessions: 16 },
        }
    }
}

impl Accelerator for Simulated {
    fn cost_model(&self) -> CostModel {
        self.cost
    }

    fn capabilities(&self) -> Caps {
        self.caps
    }

    fn submit(&mut self, work: Batch, deadline_ns: u64) -> Result<Handle, Refused> {
        let needed = self.cost.ns_per_token.saturating_mul(work.tokens as u64);
        if needed > deadline_ns {
            return Err(Refused::DeadlineUnreachable);
        }
        if self.in_flight >= self.caps.max_concurrent_sessions {
            return Err(Refused::NoThroughputReserve);
        }
        self.in_flight += 1;
        let h = Handle(self.next_handle);
        self.next_handle += 1;
        Ok(h)
    }

    fn cancel(&mut self, _h: Handle) {
        self.in_flight = self.in_flight.saturating_sub(1);
    }
}
