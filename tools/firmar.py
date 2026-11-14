"""Firma el ejecutable para las actualizaciones automáticas.

Escribe junto al .exe un archivo .sig con la firma Ed25519 (en base64) de un
manifiesto que une la versión y el SHA-256 del archivo. La app lleva dentro la
clave pública y rechaza cualquier descarga cuya firma no coincida, o cuya
versión no sea posterior a la instalada.

Uso (en GitHub Actions): UPDATE_SIGNING_KEY=<PEM> python tools/firmar.py <exe>
"""
import base64
import hashlib
import json
import os
import sys

from cryptography.hazmat.primitives import serialization


def manifest(version: str, digest: str) -> bytes:
    # Debe coincidir exactamente con `updater::manifest` en Rust.
    return f"FortniteChestCounter update\nversion={version}\nsha256={digest}\n".encode()


def main() -> None:
    exe = sys.argv[1]
    version = json.load(open("src-tauri/tauri.conf.json", encoding="utf-8"))["version"]
    with open(exe, "rb") as f:
        digest = hashlib.sha256(f.read()).hexdigest()
    key = serialization.load_pem_private_key(os.environ["UPDATE_SIGNING_KEY"].encode(), None)
    signature = key.sign(manifest(version, digest))
    with open(exe + ".sig", "w", encoding="ascii", newline="\n") as f:
        f.write(base64.b64encode(signature).decode() + "\n")
    print(f"firmado {os.path.basename(exe)} v{version} sha256={digest[:16]}...")


if __name__ == "__main__":
    main()
