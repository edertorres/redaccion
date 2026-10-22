# Despliegue en Coolify — DesignCraft Web / Redacción Editorial

Esta guía detalla los pasos para desplegar la plataforma editorial **DesignCraft Web** en un servidor gestionado con **Coolify**.

---

## 1. Arquitectura de Despliegue

La aplicación se compone de:
- **Frontend WebAssembly (WASM):** Aplicación cliente de alto rendimiento renderizada con WebGL2/WebGPU en navegador.
- **Micro-Backend Nativo (`designcraft-cli serve`):** Servidor HTTP en Rust que entrega los estáticos compilados y expone las APIs editoriales (`/api/cloud/*`, `/api/documents`, `/api/templates`, control de concurrencia y previsualizaciones PDF).
- **Volúmenes Persistentes:**
  - `/app/documents`: Almacena borradores y documentos `.designcraft` en curso.
  - `/app/templates`: Almacena plantillas base y el catálogo sincronizado.

---

## 2. Paso a Paso en Coolify

### Paso 1: Crear Nueva Aplicación
1. En el panel de Coolify, ingresa a tu **Project / Environment**.
2. Haz clic en **+ New** ▸ **Application** ▸ **Public Repository** o **Private Repository (GitHub App / Deploy Key)**.
3. Ingresa la URL del repositorio:
   ```text
   https://github.com/edertorres/redaccion
   ```
4. Rama: `main`.

### Paso 2: Tipo de Construcción (Build Pack)
- Selecciona **Docker Compose** (detectará automáticamente `docker-compose.yml`) o **Dockerfile**.
- Si seleccionas **Dockerfile**:
  - Puerto de destino (Port): `8767`.
  - Ruta de Health Check: `/api/health`.

### Paso 3: Configurar Variables de Entorno (Environment Variables)
En la pestaña **Environment Variables** de tu aplicación en Coolify, copia y completa las variables necesarias según tu proveedor de almacenamiento:

```ini
PORT=8767
BIND_ADDR=0.0.0.0

# Motores de IA (Opcional)
GEMINI_API_KEY=
VITE_DEEPSEEK_API_KEY=

# Google Drive (Requerido para integración con Google Cloud)
VITE_GOOGLE_DRIVE_CLIENT_ID=tu_client_id.apps.googleusercontent.com
VITE_GOOGLE_DRIVE_API_KEY=tu_api_key
VITE_GOOGLE_DRIVE_CLIENT_SECRET=tu_client_secret
VITE_DRIVE_TEMPLATES_FOLDER_ID=id_carpeta_plantillas
VITE_DRIVE_DESTINATION_FOLDER_ID=id_carpeta_destino
VITE_GOOGLE_SHEETS_LOG_ID=id_hoja_calculo_opcional
VITE_STORAGE_MODE=google

# Nextcloud (Opcional si usas Nextcloud)
VITE_NEXTCLOUD_URL=https://tu-nextcloud.com
VITE_NEXTCLOUD_CLIENT_ID=
VITE_NEXTCLOUD_CLIENT_SECRET=
VITE_NEXTCLOUD_TEMPLATES_PATH=/Flujo_editorial/Plantillas
VITE_NEXTCLOUD_DESTINATION_PATH=/Flujo_editorial/Redaccion
```

> [!NOTE]
> Las variables sensibles se mantienen exclusivamente en Coolify y nunca se almacenan en el repositorio Git.

### Paso 4: Configuración de Dominio y SSL
1. En la pestaña **General Settings** de Coolify, asigna el dominio público (ejemplo: `https://redaccion.tudominio.com`).
2. Coolify y Traefik generarán y renovarán automáticamente los certificados SSL con Let's Encrypt.

### Paso 5: Desplegar
Haz clic en **Deploy**. Coolify compilará el código WebAssembly, compilará el backend en Rust e iniciará el contenedor con monitoreo de salud activo en `/api/health`.
