# GlintOS

Um microkernel em Rust no qual **inferência, KV cache e aceleradores são recursos de primeira classe do núcleo**.

Os sistemas operacionais atuais não controlam o recurso que importa para IA: o acelerador é um dispositivo opaco atrás de um driver, e o KV cache — o maior objeto de memória do sistema, com padrão de acesso conhecido e política de descarte calculável — é invisível ao gerenciador de memória. O GlintOS coloca os dois dentro do núcleo.

> Estado: **marco M0**. Boota em bare metal, assume o console, mapeia a memória física e exerce o controle de admissão. O motor de inferência chega no M1.

Arquitetura completa em [`docs/arquitetura-v0.1.md`](docs/arquitetura-v0.1.md).

---

## Construir e rodar (Windows 11)

O caminho é o **WSL2 com Ubuntu** — nele tudo abaixo funciona sem adaptação. No PowerShell, como administrador:

```powershell
wsl --install -d Ubuntu
```

Reinicie, abra o Ubuntu e rode:

```bash
# toolchain
sudo apt update
sudo apt install -y build-essential git xorriso qemu-system-x86 ovmf
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

# projeto
git clone https://github.com/charlesmmorais/GlintOS.git
cd GlintOS
make run
```

O `rust-toolchain.toml` instala o alvo `x86_64-unknown-none` sozinho no primeiro `cargo build`. **Não é preciso nightly** — o alvo é tier-2 estável, com `core` e `alloc` pré-compilados, e o kernel não usa nenhuma feature instável.

### O que você deve ver

A saída do kernel sai pela **serial**, no próprio terminal:

```
  GlintOS 0.1.0 -- marco M0
  inferencia, KV cache e aceleradores como recursos de nucleo

[mem]  hhdm     0xffff800000000000
[mem]  fisica   ... MiB livres em N regioes
[mem]  heap     ... KiB usados / ... KiB livres
[fb]   1024x768 -- sinal de boot desenhado
[accel] simulado: 8000000 ns/token, teto 240 tok/s

[sched] sessao 1 ADMITIDA   128 tokens, reserva 100 tok/s, uso 41%
[sched] sessao 2 ADMITIDA   64 tokens, reserva 100 tok/s, uso 83%
[sched] sessao 3 RECUSADA   sem reserva de vazao disponivel no dispositivo

[journal] 4 eventos registrados
```

A terceira linha é a tese inteira em miniatura: **carga que não cabe é recusada na entrada, com motivo legível** — nunca aceita para morrer depois.

E **a mesma saida aparece na tela**, no console de texto do framebuffer: barra de identidade no topo e o log do kernel abaixo, com rolagem. Em hardware real, onde nao ha serial visivel, essa e' a unica forma de ver o que a maquina esta fazendo.

### Gravar no pendrive

```bash
make            # gera glintos.iso
```

No Windows, grave o `glintos.iso` com o **Rufus** em modo *DD Image*. A ISO é híbrida — boota por BIOS e por UEFI. Use `-serial stdio` só no QEMU; em hardware real, a saída vai para a porta serial física (ou, a partir do M2, para o framebuffer).

---

## Estrutura

```
kernel/src/
  main.rs          entrada, requisições ao Limine, sequência de boot
  boot/limine.rs   protocolo Limine escrito à mão (mantém o kernel em estável)
  sync.rs          spinlock próprio (sem dependência externa)
  arch/x86_64/     I/O de porta, halt
  console/         serial 16550 + console de texto no framebuffer (fonte 8x16)
  mem/             alocador de quadros físicos + heap do kernel
  session/         a unidade de execução: pesos + KV + orçamento + prazo
  kv/              KV cache paginado como objeto de kernel
  sched/           escalonador por reserva + controle de admissão
  accel/           trait Accelerator + driver Simulated
  journal/         registro append-only de toda decisão
```

`session`, `kv`, `sched`, `accel` e `journal` estão como **esqueleto com os tipos e as políticas declarados** — compilam e a admissão já é real. O preenchimento segue os marcos abaixo.

## Marcos

| | Entrega | Pronto quando |
|---|---|---|
| **M0** | Boot, console (serial + framebuffer), memória, admissão | ✅ |
| **M1** | `llama2.c` portado para `no_std`, uma sessão | Gera tokens, KV alocado pelo kernel |
| **M2** | Duas sessões, escalonador preemptivo, `ps`/`top` | Preempção visível ao vivo |
| **M3** | Orçamento, despejo de KV sob pressão | Terceira sessão recusada por memória |
| **M4** | Journal, `why`, boot de pendrive em hardware | Roda fora do QEMU |
| **M5** | COW de prefixo, camadas de memória | Ganho medido |
| **M6** | Segundo driver de acelerador | Mesma interface, hardware diferente |

## Estado da verificação

O código **passa por `cargo check` e `cargo clippy` sem erros nem avisos** (verificado contra o alvo do host, que exercita todo o type-checking; o alvo `x86_64-unknown-none` difere apenas na geração de código e na ligação).

Não foi possível ligar a imagem nem bootar no ambiente onde foi escrito, então dois pontos permanecem não exercitados:

1. **Branch do Limine.** O Makefile usa `v9.x-binary`. Se o clone falhar: `make LIMINE_BRANCH=v8.x-binary`.
2. **Formato do `limine.conf`.** A sintaxe `path: boot():/boot/glint` é do Limine 8+. Em versões anteriores era `limine.cfg` com `KERNEL_PATH=`.

### Por que o protocolo Limine é escrito à mão

A crate `limine` exige nightly (`feature(ptr_metadata)`). Como o protocolo do M0 é um punhado de structs `repr(C)` e magic numbers publicados, ele vive em `kernel/src/boot/limine.rs` — o kernel fica em **Rust estável** e a única dependência externa é o alocador de heap.

## Decisão que vem logo: ponto flutuante

O alvo `x86_64-unknown-none` desabilita SSE e usa *soft-float* — correto para um kernel, **inviável para inferência**. No M1 será preciso habilitar SSE/AVX em `CR0`/`CR4` no boot e ajustar `target-feature`, além de decidir a política de salvamento de estado de FPU na preempção. Está registrado nas decisões em aberto do documento de arquitetura.

## Licença

Apache-2.0.
