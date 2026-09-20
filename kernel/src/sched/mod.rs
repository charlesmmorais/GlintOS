//! Escalonador por reserva e controle de admissao.
//!
//! Por reserva, NAO por prioridade: prioridade e' o mecanismo pelo qual se
//! inventa inversao de prioridade (secao 4.2).
//!
//! ESQUELETO: a admissao ja e' real; a fila de execucao chega no M2,
//! junto com a fonte de tempo (timer APIC).

use crate::accel::{Accelerator, CostModel};
use crate::session::{Budget, Refused};

pub struct Scheduler {
    /// Soma das reservas ja concedidas, em tokens/s.
    reserved_tps: u32,
    /// Teto do dispositivo.
    capacity_tps: u32,
    admitted: u32,
}

impl Scheduler {
    pub const fn new() -> Self {
        Self { reserved_tps: 0, capacity_tps: 0, admitted: 0 }
    }

    pub fn bind<A: Accelerator>(&mut self, accel: &A) {
        self.capacity_tps = accel.cost_model().max_tokens_per_sec;
    }

    /// Controle de admissao.
    ///
    /// Principio 3: recusar e' melhor que enfileirar. Uma carga que nao cabe
    /// e' negada AQUI, com motivo -- nunca aceita para morrer depois (I4).
    pub fn admit(
        &mut self,
        budget: &Budget,
        deadline_ns: Option<u64>,
        want_tps: u32,
        cost: &CostModel,
    ) -> Result<u32, Refused> {
        if let Some(deadline) = deadline_ns {
            let needed = cost.ns_per_token.saturating_mul(budget.tokens_left() as u64);
            if needed > deadline {
                return Err(Refused::DeadlineUnreachable);
            }
        }
        if self.reserved_tps + want_tps > self.capacity_tps {
            return Err(Refused::NoThroughputReserve);
        }
        self.reserved_tps += want_tps;
        self.admitted += 1;
        Ok(want_tps)
    }

    pub fn release(&mut self, reserved_tps: u32) {
        self.reserved_tps = self.reserved_tps.saturating_sub(reserved_tps);
        self.admitted = self.admitted.saturating_sub(1);
    }

    pub fn utilization_pct(&self) -> u32 {
        if self.capacity_tps == 0 {
            return 0;
        }
        self.reserved_tps.saturating_mul(100) / self.capacity_tps
    }

    pub fn admitted(&self) -> u32 {
        self.admitted
    }
}
