#!/usr/bin/env python3
"""
DesignCraft Batch Style Importer / Importador de Estilos en Lote
================================================================
Escanea recursivamente una carpeta en busca de archivos .idml, importa y
sobreescribe los estilos de párrafo y carácter desde un archivo base .designcraft,
y guarda el resultado como un archivo nativo .designcraft junto al .idml original.

Uso:
    python3 scripts/batch_apply_styles.py <CARPETA> --base <ARCHIVO_BASE.designcraft> [OPCIONES]

Ejemplos:
    # Aplicar estilos de general.designcraft a todos los IDML en templates/:
    python3 scripts/batch_apply_styles.py templates/ --base templates/general.designcraft

    # Procesar con 8 hilos en paralelo:
    python3 scripts/batch_apply_styles.py /ruta/edicion/ --base base.designcraft --jobs 8

    # Simulación previa sin escribir cambios:
    python3 scripts/batch_apply_styles.py /ruta/edicion/ --base base.designcraft --dry-run
"""

import os
import sys
import json
import time
import shutil
import argparse
import subprocess
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor, as_completed

# Colores ANSI para terminal
GREEN = "\033[92m"
BLUE = "\033[94m"
YELLOW = "\033[93m"
RED = "\033[91m"
BOLD = "\033[1m"
DIM = "\033[2m"
CYAN = "\033[96m"
RESET = "\033[0m"


def find_cli_binary(custom_path=None):
    """Localiza el binario ejecutable de designcraft-cli."""
    if custom_path:
        p = Path(custom_path).resolve()
        if p.is_file() and os.access(p, os.X_OK):
            return str(p)
        print(f"{YELLOW}Advertencia: Binario CLI especificado no existe o no es ejecutable: {p}{RESET}")

    script_dir = Path(__file__).resolve().parent
    repo_root = script_dir.parent

    candidates = [
        repo_root / "target" / "release" / "designcraft-cli",
        repo_root / "target" / "debug" / "designcraft-cli",
        Path.home() / ".cargo" / "bin" / "designcraft-cli",
    ]

    for candidate in candidates:
        if candidate.is_file() and os.access(candidate, os.X_OK):
            return str(candidate)

    which_cli = shutil.which("designcraft-cli")
    if which_cli:
        return which_cli

    return None


def process_single_file(cli_bin, idml_path, base_path, kinds, overwrite=True, dry_run=False):
    """
    Abre un archivo .idml, importa los estilos del archivo base y guarda el .designcraft resultante.
    """
    idml_path = Path(idml_path).resolve()
    base_path = Path(base_path).resolve()
    out_dc = idml_path.with_suffix(".designcraft")

    if out_dc.exists() and not overwrite:
        return {
            "status": "skipped",
            "path": idml_path,
            "out": out_dc,
            "message": "Archivo .designcraft ya existe (usa --overwrite para reemplazar)",
        }

    if dry_run:
        return {
            "status": "dry_run",
            "path": idml_path,
            "out": out_dc,
            "message": "Simulación: importaría estilos de párrafo y carácter",
        }

    # Comando style.load serializado en JSON
    style_cmd_params = {
        "path": str(base_path),
        "kinds": kinds,
        "replace": True,
    }
    style_cmd_str = f"style.load={json.dumps(style_cmd_params, separators=(',', ':'))}"

    cmd = [
        cli_bin,
        "run",
        "--in", str(idml_path),
        "--cmd", style_cmd_str,
        "--export", str(out_dc),
    ]

    t0 = time.time()
    try:
        proc = subprocess.run(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=120,
        )
        elapsed = time.time() - t0

        if proc.returncode != 0:
            err_msg = proc.stderr.strip() or proc.stdout.strip() or f"Código de salida: {proc.returncode}"
            return {
                "status": "error",
                "path": idml_path,
                "out": out_dc,
                "error": err_msg,
                "time": elapsed,
            }

        # Analizar respuesta JSON del comando style.load
        stats = {}
        for line in proc.stdout.splitlines():
            line = line.strip()
            if line.startswith("{") and "replaced" in line:
                try:
                    stats = json.loads(line)
                    break
                except Exception:
                    pass

        return {
            "status": "ok",
            "path": idml_path,
            "out": out_dc,
            "size": out_dc.stat().st_size if out_dc.exists() else 0,
            "time": elapsed,
            "stats": stats,
        }

    except subprocess.TimeoutExpired:
        return {
            "status": "error",
            "path": idml_path,
            "out": out_dc,
            "error": "Tiempo de espera agotado (timeout > 120s)",
            "time": 120,
        }
    except Exception as e:
        return {
            "status": "error",
            "path": idml_path,
            "out": out_dc,
            "error": str(e),
            "time": time.time() - t0,
        }


def find_idml_files(root_dir):
    """Encuentra recursivamente todos los archivos .idml válidos."""
    root_path = Path(root_dir).resolve()
    if not root_path.is_dir():
        return []

    files = []
    for p in root_path.rglob("*.[iI][dD][mM][lL]"):
        # Ignorar archivos temporales o de sistema
        if p.name.startswith(".") or p.name.startswith("~") or ".backups" in p.parts or ".git" in p.parts:
            continue
        if p.is_file():
            files.append(p)

    return sorted(files)


def format_size(bytes_num):
    for unit in ["B", "KB", "MB", "GB"]:
        if bytes_num < 1024.0:
            return f"{bytes_num:.1f} {unit}"
        bytes_num /= 1024.0
    return f"{bytes_num:.1f} TB"


def main():
    parser = argparse.ArgumentParser(
        description="Aplica y sobreescribe estilos de párrafo y carácter desde un archivo base .designcraft a archivos .idml recursivamente.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Ejemplo de uso:
  python3 scripts/batch_apply_styles.py /ruta/a/carpeta/ediciones --base /ruta/a/estilos_base.designcraft
  python3 scripts/batch_apply_styles.py templates/ --base templates/general.designcraft --jobs 6
        """,
    )

    parser.add_argument("folder", help="Carpeta raíz a escanear recursivamente.")
    parser.add_argument("--base", "-b", required=True, help="Ruta al archivo base .designcraft que contiene los estilos de referencia.")
    parser.add_argument("--no-overwrite", action="store_true", help="No sobreescribir el archivo .designcraft si ya existe.")
    parser.add_argument("--jobs", "-j", type=int, default=4, help="Número de conversiones concurrentes en paralelo (por defecto: 4).")
    parser.add_argument("--cli", default=None, help="Ruta personalizada al ejecutable designcraft-cli.")
    parser.add_argument("--dry-run", action="store_true", help="Simula el proceso sin escribir ningún archivo.")
    parser.add_argument("--kinds", default="paragraph,character", help="Familias de estilos a importar separadas por coma (por defecto: 'paragraph,character').")

    args = parser.parse_args()

    # Validar directorio de origen
    target_dir = Path(args.folder).resolve()
    if not target_dir.exists() or not target_dir.is_dir():
        print(f"{RED}Error: La carpeta especificada no existe: {target_dir}{RESET}")
        sys.exit(1)

    # Validar archivo base
    base_file = Path(args.base).resolve()
    if not base_file.exists() or not base_file.is_file():
        print(f"{RED}Error: El archivo base especificado no existe: {base_file}{RESET}")
        sys.exit(1)

    # Localizar designcraft-cli
    cli_bin = find_cli_binary(args.cli)
    if not cli_bin:
        print(f"{RED}Error: No se encontró el binario ejecutable de `designcraft-cli`.{RESET}")
        print("Compílalo previamente con: cargo build -p designcraft-cli")
        sys.exit(1)

    kinds_list = [k.strip().lower() for k in args.kinds.split(",") if k.strip()]

    print(f"\n{BOLD}{CYAN}=== DesignCraft Batch Style Importer ==={RESET}")
    print(f"Carpeta objetivo : {BLUE}{target_dir}{RESET}")
    print(f"Archivo base     : {GREEN}{base_file.name}{RESET} ({base_file})")
    print(f"Estilos a copiar : {BOLD}{', '.join(kinds_list)}{RESET}")
    print(f"Modo reemplazo   : {BOLD}{'Sí (sobreescribe estilos existentes)' if not args.no_overwrite else 'No'}{RESET}")
    print(f"Motor CLI        : {DIM}{cli_bin}{RESET}")
    if args.dry_run:
        print(f"{YELLOW}[MODO SIMULACIÓN - DRY RUN ACTIVO]{RESET}")
    print()

    # Buscar archivos .idml
    files = find_idml_files(target_dir)
    if not files:
        print(f"{YELLOW}No se encontraron archivos .idml en: {target_dir}{RESET}")
        sys.exit(0)

    print(f"Se encontraron {BOLD}{len(files)}{RESET} archivo(s) .idml a procesar.\n")

    t_start = time.time()
    results = []
    overwrite = not args.no_overwrite

    # Ejecución paralela
    max_workers = max(1, min(args.jobs, len(files)))
    with ThreadPoolExecutor(max_workers=max_workers) as executor:
        futures = {
            executor.submit(
                process_single_file,
                cli_bin,
                f,
                base_file,
                kinds_list,
                overwrite=overwrite,
                dry_run=args.dry_run,
            ): f
            for f in files
        }

        count = 0
        for future in as_completed(futures):
            count += 1
            res = future.result()
            results.append(res)

            rel_in = res["path"].relative_to(target_dir) if res["path"].is_relative_to(target_dir) else res["path"].name
            rel_out = res["out"].relative_to(target_dir) if res["out"].is_relative_to(target_dir) else res["out"].name

            prefix = f"[{count}/{len(files)}]"

            if res["status"] == "ok":
                stats = res.get("stats", {})
                rep_p = stats.get("replaced", {}).get("paragraph", 0)
                add_p = stats.get("added", {}).get("paragraph", 0)
                rep_c = stats.get("replaced", {}).get("character", 0)
                add_c = stats.get("added", {}).get("character", 0)

                style_summary = f"(Párrafo: +{add_p}/↺{rep_p}, Carácter: +{add_c}/↺{rep_c})"
                print(
                    f"{GREEN}✓{RESET} {prefix} {BOLD}{rel_in}{RESET} → {rel_out} "
                    f"{DIM}{format_size(res['size'])} en {res['time']:.2f}s{RESET} {CYAN}{style_summary}{RESET}"
                )
            elif res["status"] == "skipped":
                print(f"{YELLOW}⊘{RESET} {prefix} {DIM}{rel_in}: {res['message']}{RESET}")
            elif res["status"] == "dry_run":
                print(f"{BLUE}ℹ{RESET} {prefix} [DRY-RUN] {rel_in} → {rel_out}")
            else:
                print(f"{RED}✗{RESET} {prefix} {BOLD}{rel_in}{RESET}: {RED}{res.get('error', 'Error desconocido')}{RESET}")

    total_time = time.time() - t_start

    # Resumen final
    ok_count = sum(1 for r in results if r["status"] == "ok")
    err_count = sum(1 for r in results if r["status"] == "error")
    skip_count = sum(1 for r in results if r["status"] == "skipped")

    print(f"\n{BOLD}{CYAN}=== Resumen del Proceso ==={RESET}")
    print(f"Total procesados : {BOLD}{len(files)}{RESET}")
    print(f"Completados con éxito: {GREEN}{ok_count}{RESET}")
    if skip_count > 0:
        print(f"Omitidos             : {YELLOW}{skip_count}{RESET}")
    if err_count > 0:
        print(f"Errores              : {RED}{err_count}{RESET}")
    print(f"Tiempo total         : {total_time:.2f} segundos")
    print()

    if err_count > 0:
        sys.exit(1)


if __name__ == "__main__":
    main()
