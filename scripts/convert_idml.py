#!/usr/bin/env python3
"""
DesignCraft Batch IDML Converter / Convertidor de IDML a DesignCraft en Lote
===========================================================================
Busca y convierte recursivamente archivos de Adobe InDesign (.idml) a formato
nativo de DesignCraft (.designcraft), con generación opcional de miniaturas PNG.

Uso:
    python3 scripts/convert_idml.py [RUTA_ORIGEN] [OPCIONES]
    ./scripts/convert_idml.sh [RUTA_ORIGEN] [OPCIONES]

Ejemplos:
    # Convertir todas las plantillas en templates/ en el mismo lugar:
    python3 scripts/convert_idml.py templates/

    # Convertir recursivamente hacia otra carpeta de destino:
    python3 scripts/convert_idml.py /ruta/indesign/ --output-dir plantillas_designcraft/

    # Sobreescribir existentes y usar 8 procesos en paralelo:
    python3 scripts/convert_idml.py templates/ --overwrite --jobs 8

    # Simular conversión sin escribir archivos:
    python3 scripts/convert_idml.py templates/ --dry-run
"""

import os
import sys
import argparse
import subprocess
import time
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor, as_completed

# Colores para la consola
GREEN = "\033[92m"
BLUE = "\033[94m"
YELLOW = "\033[93m"
RED = "\033[91m"
BOLD = "\033[1m"
DIM = "\033[2m"
RESET = "\033[0m"


def find_cli_binary(custom_path=None):
    """Localiza el binario de designcraft-cli."""
    if custom_path:
        p = Path(custom_path).resolve()
        if p.is_file() and os.access(p, os.X_OK):
            return str(p)
        print(f"{YELLOW}Advertencia: Binario CLI especificado no existe o no es ejecutable: {p}{RESET}")

    # Buscar en el árbol del proyecto
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

    # Si está en el PATH del sistema
    import shutil
    which_cli = shutil.which("designcraft-cli")
    if which_cli:
        return which_cli

    return None


def convert_single_idml(cli_bin, idml_path, output_designcraft_path, make_thumbnail=False, thumb_scale=0.5, overwrite=False, dry_run=False):
    """
    Convierte un único archivo .idml a .designcraft puro y nativo.
    """
    idml_path = Path(idml_path).resolve()
    out_dc = Path(output_designcraft_path).resolve()
    out_png = out_dc.with_suffix(".png")

    if out_dc.exists() and not overwrite:
        return {
            "status": "skipped",
            "path": idml_path,
            "out": out_dc,
            "message": "Ya existe (usa --overwrite para reemplazar)",
        }

    if dry_run:
        return {
            "status": "dry_run",
            "path": idml_path,
            "out": out_dc,
            "thumb": out_png if make_thumbnail else None,
            "message": "Simulación de conversión",
        }

    # Asegurar que el directorio de salida existe
    out_dc.parent.mkdir(parents=True, exist_ok=True)

    # Armar comando designcraft-cli run
    # Si make_thumbnail es True, exporta el .designcraft y el .png en la misma ejecución
    cmd = [
        cli_bin,
        "run",
        "--in", str(idml_path),
        "--export", str(out_dc),
    ]

    if make_thumbnail:
        cmd.extend([
            "--page", "0",
            "--scale", str(thumb_scale),
            "--export", str(out_png),
        ])

    t0 = time.time()
    try:
        proc = subprocess.run(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
            timeout=120,
        )
        elapsed = time.time() - t0

        if proc.returncode == 0 and out_dc.exists():
            return {
                "status": "success",
                "path": idml_path,
                "out": out_dc,
                "thumb": out_png if make_thumbnail and out_png.exists() else None,
                "time": elapsed,
            }
        else:
            err_msg = proc.stderr.strip() or proc.stdout.strip() or f"Código de salida {proc.returncode}"
            return {
                "status": "error",
                "path": idml_path,
                "out": out_dc,
                "error": err_msg,
                "time": elapsed,
            }
    except subprocess.TimeoutExpired:
        return {
            "status": "error",
            "path": idml_path,
            "out": out_dc,
            "error": "Tiempo de espera agotado (>120s)",
            "time": 120.0,
        }
    except Exception as ex:
        return {
            "status": "error",
            "path": idml_path,
            "out": out_dc,
            "error": str(ex),
            "time": time.time() - t0,
        }


def collect_idml_files(source_dir):
    """Busca recursivamente todos los archivos .idml."""
    source_path = Path(source_dir).resolve()
    if source_path.is_file():
        if source_path.suffix.lower() == ".idml":
            return [source_path]
        return []

    collected = []
    for root, _, files in os.walk(source_path):
        for f in files:
            if f.lower().endswith(".idml") and not f.startswith("."):
                collected.append(Path(root) / f)

    collected.sort()
    return collected


def main():
    parser = argparse.ArgumentParser(
        description="Convertidor recursivo en lote de archivos InDesign IDML a DesignCraft.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    parser.add_argument(
        "source",
        nargs="?",
        default="templates",
        help="Directorio o archivo .idml a convertir (por defecto: ./templates)",
    )
    parser.add_argument(
        "-o", "--output-dir",
        default=None,
        help="Directorio destino (si no se especifica, se guardan junto al .idml original)",
    )
    parser.add_argument(
        "--thumbnails",
        dest="thumbnails",
        action="store_true",
        default=False,
        help="Generar opcionalmente miniaturas PNG (desactivado por defecto para máxima velocidad)",
    )
    parser.add_argument(
        "--thumbnail-scale",
        type=float,
        default=0.5,
        help="Escala de renderizado para miniaturas PNG (defecto: 0.5)",
    )
    parser.add_argument(
        "-f", "--overwrite",
        action="store_true",
        default=False,
        help="Sobreescribir archivos .designcraft y .png existentes",
    )
    parser.add_argument(
        "-j", "--jobs",
        type=int,
        default=max(1, (os.cpu_count() or 4) // 2),
        help="Número de conversiones en paralelo (defecto: mitad de CPUs disponibles)",
    )
    parser.add_argument(
        "--cli-path",
        default=None,
        help="Ruta manual al ejecutable designcraft-cli",
    )
    parser.add_argument(
        "-n", "--dry-run",
        action="store_true",
        default=False,
        help="Mostrar qué archivos se convertirían sin realizar cambios",
    )

    args = parser.parse_args()

    # 1. Localizar binario CLI
    cli_bin = find_cli_binary(args.cli_path)
    if not cli_bin and not args.dry_run:
        print(f"{RED}{BOLD}Error:{RESET} No se encontró el binario `designcraft-cli`.")
        print("Por favor compílalo con:")
        print(f"  {BLUE}cargo build --release -p designcraft-cli{RESET}")
        sys.exit(1)

    # 2. Recolectar archivos IDML
    source_path = Path(args.source).resolve()
    if not source_path.exists():
        print(f"{RED}{BOLD}Error:{RESET} La ruta de origen no existe: {source_path}")
        sys.exit(1)

    print(f"\n{BOLD}============================================================{RESET}")
    print(f"{BOLD}  DesignCraft — Conversión Recursiva de Plantillas IDML{RESET}")
    print(f"{BOLD}============================================================{RESET}")
    print(f"  • Origen:      {BLUE}{source_path}{RESET}")
    if args.output_dir:
        out_root = Path(args.output_dir).resolve()
        print(f"  • Destino:     {BLUE}{out_root}{RESET}")
    else:
        out_root = None
        print(f"  • Destino:     {DIM}En el mismo directorio de cada archivo{RESET}")
    print(f"  • Miniaturas:  {'Sí (PNG a escala ' + str(args.thumbnail_scale) + ')' if args.thumbnails else 'No'}")
    print(f"  • Sobrescribir:{'Sí' if args.overwrite else 'No (omite existentes)'}")
    print(f"  • Paralelismo: {args.jobs} hilo(s)")
    if cli_bin:
        print(f"  • Binario CLI: {DIM}{cli_bin}{RESET}")
    if args.dry_run:
        print(f"  • {YELLOW}MODO SIMULACIÓN (Dry Run) — Ningún archivo será modificado{RESET}")
    print("------------------------------------------------------------\n")

    idml_files = collect_idml_files(source_path)
    total_files = len(idml_files)

    if total_files == 0:
        print(f"{YELLOW}No se encontraron archivos .idml en `{source_path}`.{RESET}\n")
        sys.exit(0)

    print(f"🔍 Encontrados {BOLD}{total_files}{RESET} archivo(s) .idml para procesar.\n")

    # 3. Preparar lista de tareas
    tasks = []
    for idml_file in idml_files:
        if out_root:
            if source_path.is_dir():
                rel = idml_file.relative_to(source_path)
                target_dc = (out_root / rel).with_suffix(".designcraft")
            else:
                target_dc = (out_root / idml_file.name).with_suffix(".designcraft")
        else:
            target_dc = idml_file.with_suffix(".designcraft")

        tasks.append((idml_file, target_dc))

    # 4. Procesar en lote (multiproceso / multihilo)
    start_total_time = time.time()
    results = {"success": 0, "skipped": 0, "error": 0, "dry_run": 0}
    errors_list = []

    with ThreadPoolExecutor(max_workers=args.jobs) as executor:
        futures = {
            executor.submit(
                convert_single_idml,
                cli_bin,
                idml_file,
                target_dc,
                make_thumbnail=args.thumbnails,
                thumb_scale=args.thumbnail_scale,
                overwrite=args.overwrite,
                dry_run=args.dry_run,
            ): (idml_file, target_dc)
            for (idml_file, target_dc) in tasks
        }

        idx = 0
        for future in as_completed(futures):
            idx += 1
            res = future.result()
            status = res["status"]
            results[status] = results.get(status, 0) + 1

            rel_in = res["path"].name
            pct = int((idx / total_files) * 100)

            if status == "success":
                thumb_note = " + 🖼️ PNG" if res.get("thumb") else ""
                print(f"  [{idx:3d}/{total_files}] ({pct:2d}%) {GREEN}✓ Convertido:{RESET} {rel_in} → {res['out'].name}{thumb_note} {DIM}({res.get('time', 0):.2f}s){RESET}")
            elif status == "skipped":
                print(f"  [{idx:3d}/{total_files}] ({pct:2d}%) {YELLOW}↷ Omitido:{RESET}    {rel_in} {DIM}(ya existe){RESET}")
            elif status == "dry_run":
                print(f"  [{idx:3d}/{total_files}] ({pct:2d}%) {BLUE}ℹ Simulado:{RESET}   {rel_in} → {res['out'].name}")
            else:
                err = res.get("error", "Error desconocido")
                errors_list.append((res["path"], err))
                print(f"  [{idx:3d}/{total_files}] ({pct:2d}%) {RED}✗ Error:{RESET}      {rel_in}: {err}")

    total_elapsed = time.time() - start_total_time

    # 5. Resumen final
    print("\n------------------------------------------------------------")
    print(f"{BOLD}Resumen de Conversión:{RESET}")
    print(f"  • Total analizados: {BOLD}{total_files}{RESET}")
    print(f"  • {GREEN}Completados con éxito:{RESET} {results['success']}")
    print(f"  • {YELLOW}Omitidos (ya existían):{RESET} {results['skipped']}")
    if results['error'] > 0:
        print(f"  • {RED}Errores:{RESET}                {results['error']}")
    if results['dry_run'] > 0:
        print(f"  • {BLUE}Simulados:{RESET}              {results['dry_run']}")
    print(f"  • Tiempo total transcurrido: {total_elapsed:.2f} segundos")
    if total_files > 0 and results['success'] > 0:
        rate = results['success'] / total_elapsed
        print(f"  • Velocidad media:           {rate:.1f} páginas/segundo")
    print("============================================================\n")

    if errors_list:
        print(f"{RED}{BOLD}Detalle de errores ({len(errors_list)}):{RESET}")
        for p, err in errors_list[:10]:
            print(f"  - {p.name}: {err}")
        if len(errors_list) > 10:
            print(f"  - ... y {len(errors_list) - 10} errores más.")
        sys.exit(1)


if __name__ == "__main__":
    main()
