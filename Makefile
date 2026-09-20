# GlintOS -- build da imagem bootavel.
# Testado no caminho WSL2 (Ubuntu). Requisitos: rustup, xorriso, qemu-system-x86, git, make.

TARGET        := x86_64-unknown-none
KERNEL_BIN    := target/$(TARGET)/release/kernel
ISO           := glintos.iso
LIMINE_DIR    := .limine
# Se o clone falhar, tente v8.x-binary.
LIMINE_BRANCH ?= v9.x-binary

.PHONY: all kernel run run-uefi clean distclean usb-instructions

all: $(ISO)

kernel:
	cargo build --release

$(LIMINE_DIR):
	git clone https://github.com/limine-bootloader/limine.git \
		--branch=$(LIMINE_BRANCH) --depth=1 $(LIMINE_DIR)
	$(MAKE) -C $(LIMINE_DIR)

$(ISO): kernel $(LIMINE_DIR)
	rm -rf iso_root
	mkdir -p iso_root/boot/limine iso_root/EFI/BOOT
	cp $(KERNEL_BIN) iso_root/boot/glint
	cp limine.conf iso_root/boot/limine/
	cp $(LIMINE_DIR)/limine-bios.sys \
	   $(LIMINE_DIR)/limine-bios-cd.bin \
	   $(LIMINE_DIR)/limine-uefi-cd.bin iso_root/boot/limine/
	cp $(LIMINE_DIR)/BOOTX64.EFI iso_root/EFI/BOOT/
	xorriso -as mkisofs \
		-b boot/limine/limine-bios-cd.bin \
		-no-emul-boot -boot-load-size 4 -boot-info-table \
		--efi-boot boot/limine/limine-uefi-cd.bin \
		-efi-boot-part --efi-boot-image --protective-msdos-label \
		iso_root -o $(ISO)
	$(LIMINE_DIR)/limine bios-install $(ISO)
	@echo ""
	@echo "  imagem pronta: $(ISO)"

# Boot BIOS. A saida do kernel aparece no terminal (serial).
run: $(ISO)
	qemu-system-x86_64 -M q35 -m 512M -cdrom $(ISO) \
		-serial stdio -no-reboot -no-shutdown

# Boot UEFI. Requer o pacote ovmf instalado.
run-uefi: $(ISO)
	qemu-system-x86_64 -M q35 -m 512M -cdrom $(ISO) \
		-bios /usr/share/ovmf/OVMF.fd \
		-serial stdio -no-reboot -no-shutdown

usb-instructions:
	@echo "No Windows: grave $(ISO) com o Rufus em modo 'DD Image'."
	@echo "A ISO e' hibrida -- boota por BIOS e por UEFI."

clean:
	cargo clean
	rm -rf iso_root $(ISO)

distclean: clean
	rm -rf $(LIMINE_DIR)
