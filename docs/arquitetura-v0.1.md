# GlintOS — Documento de Arquitetura

**Versão:** 0.4 · **Data:** 2026-09-20 · **Status:** M0 implementado (esqueleto), verificado por `cargo check`/`clippy`; falta ligar e bootar

---

## 1. Tese

> Os sistemas operacionais atuais não controlam o recurso que importa para IA.

O acelerador é um dispositivo opaco atrás de um driver; quem escalona é a runtime em espaço de usuário, e o kernel é espectador. Pior: o kernel não sabe o que é um KV cache — o maior objeto de memória do sistema, com padrão de acesso conhecido e política de descarte calculável — e portanto não o gerencia.

A prova de que há valor real na mesa é o **PagedAttention**: paginação, uma ideia de SO dos anos 60, aplicada ao KV cache, multiplicou a vazão de servidores de inferência. E teve que ser implementada em espaço de usuário *porque o kernel não tem a abstração*. Há uma dúzia de ideias de SO na mesma situação.

**GlintOS é um microkernel em que inferência, KV cache e aceleradores são recursos de primeira classe do núcleo.**

### 1.1 O que "pensado para IA" significa aqui

Três leituras possíveis. GlintOS persegue a primeira, mantém a porta aberta para a segunda e descarta a terceira por ora.

| Leitura | Descrição | Posição |
|---|---|---|
| **A. SO para rodar IA** | Acelerador, pesos e KV como recursos escalonáveis pelo núcleo | **Núcleo do projeto** |
| **B. SO que usa IA para se governar** | Políticas aprendidas (despejo, prefetch, escalonamento) dentro do kernel | Interface preparada, sem implementação em v0 |
| **C. SO cuja interface de chamada é semântica** | Programas expressam intenção; o SO decide o como | Fora de escopo |

---

## 2. Princípios de projeto

Estes princípios desempatam decisões. Quando houver conflito, a ordem abaixo é a ordem de precedência.

1. **Mecanismo no núcleo, política fora.** O kernel nunca decide *o quê*; decide *se pode* e *quando*. Mantra clássico de microkernel, levado a sério.
2. **Nenhum componente que possa alucinar está na TCB.** Se o kernel depende de um modelo para progredir, é um kernel que trava.
3. **Recusar é melhor que enfileirar.** Carga que não cabe no orçamento é negada na admissão, com motivo, e nunca aceita para morrer depois.
4. **Reinício é rotina, não último recurso.** Componentes sem estado alheio, reinicializáveis individualmente.
5. **Familiaridade é interface.** Vocabulário de SO existente (`ps`, `top`, `free`, `kill`) aplicado a inferência. Nome novo só quando o conceito é novo de verdade.
6. **O driver mora fora.** Toda decisão específica de hardware fica atrás de uma interface de capacidade. O núcleo não sabe se está falando com CPU, simulador ou GPU.

---

## 3. Modelo de recursos

As abstrações que não existem em nenhum SO atual. Esta é a contribuição central.

### 3.1 Sessão — a unidade de execução

Não é processo. É a menor coisa escalonável, orçável e preemptível do sistema.

```rust
struct Session {
    id:       SessionId,
    weights:  WeightsRef,        // referência compartilhada, imutável
    kv:       KvHandle,          // objeto de kernel, redimensionável, despejável
    budget:   Budget,            // tokens, tempo de parede, (futuro) energia
    deadline: Option<Instant>,
    state:    SessionState,
}

enum SessionState {
    Admitted, Running, Preempted, Evicted, Done, Refused(Reason),
}
```

Propriedades exigidas:

- **Preemptível em fronteira de token.** O ponto de preempção é natural e barato — nenhum estado intermediário precisa ser salvo além do que já é o KV.
- **Sem estado alheio.** Toda a durabilidade da sessão mora em objetos do kernel. Matar uma sessão nunca corrompe outra.
- **Custo previsível.** Latência por token é aproximadamente constante para um par (modelo, hardware). Este é um presente raro: permite *admission control* de verdade e garantias de latência que um SO convencional não consegue dar.

### 3.2 Pesos — objeto compartilhado imutável

Gigabytes de dados somente-leitura. Endereçados por conteúdo, residentes uma única vez, mapeados em quantas sessões precisarem. Em v0 vêm embutidos na imagem de boot.

### 3.3 KV cache — objeto de kernel

**A peça mais importante do projeto.** Paginado em blocos de tamanho fixo, com tabela de blocos por sessão — PagedAttention dentro do kernel, e não por cima dele.

Três consequências que só são naturais quando é o kernel que gerencia:

- **Despejo com modelo de custo.** A decisão de descartar compara custo de recomputação contra custo de retenção. O kernel tem ambos os números; uma biblioteca em espaço de usuário não tem.
- **Compartilhamento de prefixo por *copy-on-write*.** Duas sessões com prefixo comum compartilham blocos até divergirem. O kernel já sabe fazer COW há cinquenta anos — aplicar ao KV é reuso, não invenção.
- **Camadas.** Interface preparada para HBM / DRAM / NVMe. Em v0, só DRAM.

### 3.4 Orçamento — capacidade, não sugestão

Tokens, tempo de parede e (futuramente) energia. Sessão sem orçamento não é escalonável. Estourar orçamento gera preempção, nunca congelamento.

### 3.5 Acelerador — driver atrás de capacidade

Esta interface é a decisão arquitetural que precisa estar certa **desde a primeira linha**, porque é ela que mantém futuros distintos vivos sem reescrita.

```rust
trait Accelerator {
    fn cost_model(&self) -> CostModel;
    fn submit(&self, work: Batch, deadline: Instant) -> Result<Handle, Refused>;
    fn cancel(&self, h: Handle);
    fn capabilities(&self) -> Caps;
}
```

Implementações previstas:

| Driver | Quando | Situação |
|---|---|---|
| `Simulated` | desde já | Modelo de custo puro. Torna escalonador e admission control determinísticos e testáveis. |
| `CpuSimd` | v0 | Alvo do MVP bootável. |
| `Cuda` / `Metal` | **futuro distante** | Drivers fechados. Realisticamente exigem modo hospedado ou VFIO, não bare metal. Decisão explícita de adiar. |

---

## 4. Arquitetura do núcleo

### 4.1 Fronteira

**Dentro do kernel:** gerenciador de memória (incluindo KV), escalonador, admission control, tabela de capacidades, journal, abstração de sessão.

**Fora:** motor de inferência, drivers de acelerador, shell/CLI, qualquer política aprendida.

Meta de tamanho: manter o núcleo na faixa de ~10 kLOC, a faixa em que verificação formal no padrão seL4 deixa de ser fantasia. Não é objetivo de v0, mas é restrição de orçamento de complexidade desde já.

### 4.2 Escalonamento

**Por reserva, não por prioridade.** Prioridade é o mecanismo pelo qual se inventa inversão de prioridade. Cada sessão admitida carrega uma reserva (tokens/s) que o escalonador honra.

**Admission control na entrada.** Dado o modelo de custo do acelerador e as reservas já concedidas, uma nova sessão é aceita ou recusada com motivo legível. Esta é a diferença entre um sistema que degrada previsivelmente e um que engasga.

### 4.3 Journal

Append-only: toda decisão de admissão, escalonamento e despejo, com a razão. Entrega três coisas de uma vez:

1. Auditoria.
2. Rollback por compensação.
3. **Replay determinístico para depuração** — você não reproduz o modelo, reproduz o journal. É o que torna um sistema não-determinístico depurável, e é a diferença entre um protótipo e algo que se confia rodando sozinho.

---

## 5. Invariantes anti-travamento

Testáveis. Cada um elimina uma classe inteira de hang, e cada um vira um teste no CI.

| # | Invariante |
|---|---|
| **I1** | Nenhuma operação do kernel espera sem prazo. Todo bloqueio carrega deadline; expirar é valor de retorno normal, não exceção. |
| **I2** | Nenhuma alocação dinâmica no caminho crítico do kernel. Recursos são reservados na admissão. |
| **I3** | Toda sessão é preemptível em fronteira de token. |
| **I4** | Carga que não cabe é recusada na admissão, nunca enfileirada indefinidamente. |
| **I5** | Nenhum componente guarda estado de outro (*sem state spill*) — portanto reinício é sempre seguro. |
| **I6** | Progresso é observável: toda sessão publica heartbeat; ausência aciona watchdog. |

Complemento de desenho: **crash-only**. O único caminho de parada é o crash e o único de partida é a recuperação — o que elimina a classe "travou no desligamento gracioso" e garante que o caminho de recuperação esteja sempre exercitado.

Se I1 for escrito como *tipo* e não como disciplina de código — um canal que carrega prazo, de modo que não seja possível expressar uma espera infinita — a ausência de deadlock passa a ser estrutural.

---

## 6. IA como política

Ponto onde o "pensado para IA" se torna literal, e onde a disciplina importa mais que a ambição.

Cada ponto de política do kernel (despejo de KV, ordem de escalonamento, prefetch, estratégia de recuperação) tem quatro partes:

1. **Política determinística padrão** — sempre existe, sempre funciona.
2. **Hook opcional de política aprendida** — propõe uma decisão.
3. **Validador de invariantes** — a proposta é verificada, nunca executada cegamente.
4. **Prazo** — se o hook estourar ou falhar, cai no padrão sem drama.

Em v0 apenas a camada 1 é implementada; 2–4 existem como interface. Nada que possa alucinar entra no caminho de progresso.

---

## 7. Interface

CLI, sobre console serial e/ou framebuffer. O princípio 5 domina: quem conhece `top` entende o sistema em cinco segundos.

| Comando | O que demonstra |
|---|---|
| `glint run --deadline 200ms` | Admission control: aceita ou recusa **com motivo** |
| `glint ps` | Sessão como unidade: pid, modelo, KV residente, tok/s, orçamento |
| `glint top` | O recurso escasso é observável: ocupação, pressão de KV |
| `glint free` | Pesos e KV como objetos gerenciados, por camada |
| `glint nice` / `kill` | Sessão preemptível e cancelável |
| `glint why <pid>` | Journal: a razão de cada decisão |

Regra de interação herdada dos invariantes: **nenhum comando pende**. Um spinner que gira para sempre é a forma-UX de um travamento.

---

## 8. Segurança (registrado, adiado)

Não entra em v0, mas o desenho fica registrado porque ele influencia a fronteira do núcleo.

- **Capacidades** com atenuação na delegação (seL4 / Genode): resolvem *quem pode chamar o quê*.
- **DIFC**, rótulos de fluxo de informação (HiStar / Flume): resolvem o caso que capacidades não resolvem — o componente que legitimamente tem a capacidade mas foi induzido a usá-la. O kernel não precisa entender semântica; apenas recusa o fluxo.

Risco conhecido a enfrentar quando chegar a hora: **rótulos vazam para cima**. Se tudo fica contaminado, o sistema para de servir e alguém desliga a verificação — que é como toda arquitetura de segurança morre na prática. A mitigação empurra o desenho para unidades pequenas e de vida curta, com declassificação em fronteira estrita e esquema validado.

---

## 9. Escopo de v0

**Dentro:** boot x86_64 por UEFI/pendrive, console, gerenciador de memória, escalonador preemptivo, sessões, KV paginado no alocador do kernel, motor de inferência CPU, CLI, journal.

**Fora (explícito):** rede, disco, sistema de arquivos, SMP, multi-tenant, DIFC, GPU NVIDIA, Metal.

---

## 10. Marcos

| Marco | Entrega | Critério de pronto |
|---|---|---|
| **M0** | Boota em QEMU, console, alocador de memória, admissão | ✅ escrito; falta compilar |
| **M1** | Motor de inferência portado (`llama2.c` → `no_std`), uma sessão | Gera tokens, KV alocado pelo kernel |
| **M2** | Duas sessões concorrentes, escalonador preemptivo, `ps` / `top` | Preempção visível ao vivo |
| **M3** | Admission control, orçamento, despejo de KV sob pressão | Terceira sessão **recusada com motivo** |
| **M4** | Journal, `why`, boot real de pendrive em hardware | Roda fora do QEMU |
| **M5** | Compartilhamento de prefixo por COW, camadas de memória | Ganho medido |
| **M6** | Segundo driver de acelerador | Mesma interface, hardware diferente |

**Demo que fecha o MVP (M4):** pendrive numa máquina qualquer, duas sessões disputando CPU e KV, uma preemptada ao vivo, memória despejada sob política visível, uma terceira recusada na entrada com a razão em uma linha. Bare metal, sem Linux embaixo.

---

## 11. Decisões registradas

| # | Decisão | Razão |
|---|---|---|
| D1 | Kernel próprio em Rust estável, `no_std`, do zero | Único caminho em que a tese é literal. Superfície de driver mínima (timer, console, teclado) torna viável: sem rede, sem disco, sem pilha USB, sem FS. **Nightly não é necessário no M0** — `x86_64-unknown-none` é tier-2 estável com `core`/`alloc` pré-compilados; nightly só passa a ser conveniente no M2, pela ABI `x86-interrupt`. |
| D2 | Bootloader: **Limine**, com o protocolo escrito à mão | UEFI + BIOS, framebuffer e mapa de memória prontos, mapeamento higher-half. Padrão de facto no osdev moderno. A crate `limine` exige nightly, então o protocolo vive em `boot/limine.rs`: kernel em estável e uma dependência externa a menos — que era o maior risco do build. |
| D3 | Motor de inferência: porte de `llama2.c` | ~700 linhas, zero dependências, praticamente desenhado para bare metal. |
| D4 | Modelo de 15M–110M parâmetros, embutido na imagem | Vazão não é o ponto; a abstração é. Elimina a necessidade de FS em v0. |
| D5 | Driver `Simulated` antes de hardware | A parte difícil é escalonador + admission control + KV. Simulado os torna determinísticos e testáveis. |
| D7 | Console de texto no framebuffer antecipado do M2 para o M0 | Boot em hardware real mostrou o problema: toda a saída ia para a serial, que ninguém escuta numa máquina física. Um console que fala por um canal invisível não é console — e contraria o princípio de interface do projeto. Fonte 8x16 rasterizada do DejaVu Sans Mono Bold; o mesmo `println!` sai na serial e na tela. |
| D6 | GPU e Metal adiados para futuro distante | Drivers fechados, inviáveis em bare metal. Interface de acelerador mantém a porta aberta sem custo hoje. |

---

## 12. Decisões em aberto

- **SMP.** Núcleo único em v0 (contenção fica mais visível, e trazer APs é trabalho). Reavaliar em M5.
- **Formato de modelo.** GGUF ou formato próprio simplificado.
- **Quantização.** int8 ou q4.
- **Tamanho de bloco do KV.** Decide granularidade de despejo e de COW de prefixo.
- **Ponto flutuante.** O alvo `x86_64-unknown-none` desabilita SSE e usa *soft-float* — correto para kernel, inviável para inferência. M1 precisa habilitar SSE/AVX em `CR0`/`CR4` e decidir a política de salvamento de estado de FPU na preempção.
- **Ergonomia da declassificação** (quando DIFC entrar). Risco número um do projeto na fase de segurança.

---

## 13. Referências

- **PagedAttention** — paginação aplicada ao KV cache (vLLM, SOSP 2023)
- **PTask** — *Operating System Abstractions to Manage GPUs as Compute Devices* (SOSP 2011)
- **Gdev** — *First-Class GPU Resource Management in the Operating System* (USENIX ATC 2012)
- **LithOS** — SO para ML eficiente (2025)
- **seL4** — microkernel verificado formalmente
- **Theseus OS** — eliminação de *state spill* como propriedade de projeto
- **Hubris** (Oxide) — tarefas reinicializáveis, sem alocação dinâmica
- **MINIX 3** — *reincarnation server*, auto-recuperação
- **HiStar / Flume** — DIFC em nível de SO
- **Crash-Only Software** — Candea & Fox (HotOS 2003)
- **AIOS** (Rutgers) — referência de contraste: coloca o LLM *como* kernel; GlintOS faz o oposto
