#!/usr/bin/env bash
# Teste de fumaca do GlintOS: boota a ISO no QEMU e verifica a saida serial.
#
# Usa o proprio console do kernel como contrato: se o boot chegou ao fim,
# as linhas abaixo existem. Serve no CI e na maquina de desenvolvimento.
#
#   ./scripts/boot-test.sh [caminho-da-iso]

set -euo pipefail

ISO="${1:-glintos.iso}"
TIMEOUT="${TIMEOUT:-90}"

if [[ ! -f "$ISO" ]]; then
  echo "erro: $ISO nao existe -- rode 'make' primeiro" >&2
  exit 1
fi

# Linhas que o kernel precisa imprimir para o M0 ser considerado bom.
ESPERADO=(
  "GlintOS 0.1.0"          # assumiu o console
  "[mem]  hhdm"            # HHDM lido do Limine
  "[mem]  fisica"          # mapa de memoria parseado
  "[sched] sessao 1 ADMITIDA"
  "[sched] sessao 3 RECUSADA"   # controle de admissao negou na entrada
  "[glint] M0 concluido"   # chegou ao fim sem panico
)

verificar() {
  local log="$1" modo="$2" falhou=0

  echo "--- saida serial ($modo) ---"
  cat "$log" || true
  echo "---------------------------"

  if grep -q "\[PANICO\]" "$log"; then
    echo "FALHA ($modo): o kernel entrou em panico" >&2
    falhou=1
  fi

  for linha in "${ESPERADO[@]}"; do
    if grep -qF -- "$linha" "$log"; then
      echo "  ok    $linha"
    else
      echo "  FALTA $linha" >&2
      falhou=1
    fi
  done

  return $falhou
}

falhas=0

# --- BIOS ---------------------------------------------------------------
echo "==> boot BIOS"
rm -f serial-bios.log
# O kernel termina em halt_loop, entao o QEMU nunca sai sozinho: o timeout
# e' o fim normal do teste, nao um erro.
timeout "$TIMEOUT" qemu-system-x86_64 \
  -M q35 -m 512M -cdrom "$ISO" \
  -serial file:serial-bios.log -display none -no-reboot || true
verificar serial-bios.log BIOS || falhas=1

# --- UEFI ---------------------------------------------------------------
OVMF=""
for cand in /usr/share/ovmf/OVMF.fd /usr/share/OVMF/OVMF_CODE.fd /usr/share/qemu/OVMF.fd; do
  [[ -f "$cand" ]] && OVMF="$cand" && break
done

if [[ -z "$OVMF" ]]; then
  echo "==> boot UEFI ignorado (OVMF nao encontrado; instale o pacote ovmf)"
else
  echo "==> boot UEFI ($OVMF)"
  rm -f serial-uefi.log
  timeout "$TIMEOUT" qemu-system-x86_64 \
    -M q35 -m 512M -cdrom "$ISO" -bios "$OVMF" \
    -serial file:serial-uefi.log -display none -no-reboot || true
  verificar serial-uefi.log UEFI || falhas=1
fi

if [[ $falhas -ne 0 ]]; then
  echo
  echo "TESTE DE BOOT FALHOU" >&2
  exit 1
fi

echo
echo "TESTE DE BOOT PASSOU"
