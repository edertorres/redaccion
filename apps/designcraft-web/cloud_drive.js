/**
 * DesignCraft Cloud Drive Integration (Google Drive & Nextcloud)
 * Exposes window.DesignCraftCloud for seamless cloud production workflow.
 */

class CloudDriveManager {
  constructor() {
    this.config = null;
    this.codeClient = null;
    this.tokenClient = null;
    this.authState = {
      isAuthenticated: false,
      accessToken: null,
      user: null,
      isLoading: true,
      error: null,
    };
    this.listeners = new Set();
    this.refreshTimer = null;
    this.initPromise = this.init();
  }

  async init() {
    try {
      // 1. Fetch server config from /api/cloud/config
      const res = await fetch('/api/cloud/config');
      if (res.ok) {
        this.config = await res.json();
      }
    } catch (e) {
      console.warn('[CloudDrive] Could not fetch /api/cloud/config, checking localStorage:', e);
    }

    if (!this.config) {
      this.config = {
        clientId: localStorage.getItem('google_client_id') || '',
        clientSecret: localStorage.getItem('google_client_secret') || '',
        apiKey: localStorage.getItem('google_api_key') || '',
        destinationFolderId: localStorage.getItem('google_dest_folder_id') || '',
        templatesFolderId: localStorage.getItem('google_tmpl_folder_id') || '',
        storageMode: 'google'
      };
    }

    // 2. Check cached credentials in localStorage
    await this.checkExistingAuth();

    // 3. Setup Google Identity Services when ready
    this.setupGoogleClient();

    this.authState.isLoading = false;
    this.notify();
  }

  setupGoogleClient() {
    const clientId = this.config?.clientId;
    if (!clientId) {
      this.authState.error = 'No Google Client ID configured';
      return;
    }

    const initClient = () => {
      if (window.google?.accounts?.oauth2) {
        const scopes = 'https://www.googleapis.com/auth/drive https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile';
        
        this.tokenClient = window.google.accounts.oauth2.initTokenClient({
          client_id: clientId,
          scope: scopes,
          callback: (resp) => this.handleTokenResponse(resp),
        });
      } else {
        setTimeout(initClient, 200);
      }
    };

    initClient();
  }

  async handleCodeResponse(response) {
    if (response.error) {
      this.authState.error = response.error_description || response.error;
      this.notify();
      return;
    }
    if (response.code) {
      this.authState.isLoading = true;
      this.notify();
      try {
        const tokenRes = await fetch('https://oauth2.googleapis.com/token', {
          method: 'POST',
          headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
          body: new URLSearchParams({
            code: response.code,
            client_id: this.config.clientId,
            client_secret: this.config.clientSecret,
            redirect_uri: window.location.origin,
            grant_type: 'authorization_code',
          }),
        });
        const data = await tokenRes.json();
        if (!tokenRes.ok) throw new Error(data.error_description || data.error || 'Token exchange failed');
        await this.setSession(data.access_token, data.expires_in, data.refresh_token);
      } catch (err) {
        this.authState.error = err.message;
      } finally {
        this.authState.isLoading = false;
        this.notify();
      }
    }
  }

  async handleTokenResponse(response) {
    console.log('[CloudDrive] handleTokenResponse:', response);
    if (response.error) {
      const errMsg = response.error_description || response.error;
      console.warn('[CloudDrive] Error al conectar con Google:', errMsg, response);
      this.authState.error = errMsg;
      if (response.error === 'popup_blocked_by_browser') {
        alert('⚠️ El navegador bloqueó la ventana emergente de Google.\n\nPor favor permite las ventanas emergentes (popups) para este sitio en la barra de direcciones del navegador.');
      } else if (response.error === 'access_denied') {
        alert('⚠️ Permiso denegado: Se requiere autorizar el acceso a Google Drive para sincronizar y abrir tus plantillas.');
      } else if (response.error === 'origin_mismatch' || errMsg.includes('origin_mismatch')) {
        const altHost = window.location.hostname === '127.0.0.1' ? 'localhost' : '127.0.0.1';
        const altUrl = window.location.href.replace(window.location.hostname, altHost);
        alert(`⚠️ Error 400: origin_mismatch de Google.\n\nEstás accediendo por "${window.location.origin}", pero en Google Cloud Console está registrado "http://${altHost}:${window.location.port}".\n\nPor favor abre la app desde:\n${altUrl}`);
      } else if (response.error !== 'popup_closed_by_user') {
        alert('⚠️ Error al iniciar sesión con Google: ' + errMsg);
      }
      this.notify();
      return;

    }
    if (response.access_token) {
      console.log('[CloudDrive] Token recibido con éxito');
      await this.setSession(response.access_token, response.expires_in || 3600);
      if (window.showCloudToast) {
        window.showCloudToast({
          type: 'success',
          title: '¡Google Drive Conectado!',
          message: `Sesión iniciada correctamente como ${this.authState.user?.name || this.authState.user?.email || 'Usuario'}.`,
          duration: 4000
        });
      }
    }
  }


  async setSession(accessToken, expiresIn, refreshToken) {
    this.authState.isAuthenticated = true;
    this.authState.accessToken = accessToken;
    this.authState.error = null;

    // Fetch user info
    try {
      const userRes = await fetch('https://www.googleapis.com/oauth2/v2/userinfo', {
        headers: { Authorization: `Bearer ${accessToken}` },
      });
      if (userRes.ok) {
        this.authState.user = await userRes.json();
      }
    } catch (e) {
      console.warn('[CloudDrive] User profile fetch failed:', e);
    }

    const expiresAt = Date.now() + (expiresIn || 3600) * 1000;
    const authData = {
      accessToken,
      refreshToken: refreshToken || (this.getStoredAuth()?.refreshToken),
      expiresAt,
      user: this.authState.user,
    };

    localStorage.setItem('google_auth', JSON.stringify(authData));
    this.notify();
  }

  getStoredAuth() {
    try {
      const s = localStorage.getItem('google_auth');
      return s ? JSON.parse(s) : null;
    } catch {
      return null;
    }
  }

  async checkExistingAuth() {
    const auth = this.getStoredAuth();
    if (!auth || !auth.accessToken) {
      this.clearSession(false);
      return;
    }

    const now = Date.now();
    if (auth.expiresAt > now + 60000) {
      this.authState.isAuthenticated = true;
      this.authState.accessToken = auth.accessToken;
      this.authState.user = auth.user;

      // Background validation against Google to detect server-side revocation or expiration
      fetch('https://www.googleapis.com/oauth2/v2/userinfo', {
        headers: { Authorization: `Bearer ${auth.accessToken}` },
      })
        .then((res) => {
          if (res.status === 401) {
            console.warn('[CloudDrive] Token expired or invalid on Google servers, disconnecting session');
            this.clearSession(true);
          } else if (res.ok) {
            res.json().then((u) => {
              if (u && u.name) {
                this.authState.user = u;
                this.notify();
              }
            });
          }
        })
        .catch((e) => {
          console.warn('[CloudDrive] Background auth check network error:', e);
        });
    } else if (auth.refreshToken && this.config?.clientSecret) {
      await this.refreshAccessToken(auth.refreshToken);
    } else {
      this.clearSession(false);
    }
  }

  async refreshAccessToken(refreshToken) {
    if (!this.config?.clientSecret || !this.config?.clientId) return;
    try {
      const res = await fetch('https://oauth2.googleapis.com/token', {
        method: 'POST',
        headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
        body: new URLSearchParams({
          client_id: this.config.clientId,
          client_secret: this.config.clientSecret,
          refresh_token: refreshToken,
          grant_type: 'refresh_token',
        }),
      });
      const data = await res.json();
      if (res.ok) {
        await this.setSession(data.access_token, data.expires_in, refreshToken);
      } else {
        this.clearSession(true);
      }
    } catch (e) {
      console.error('[CloudDrive] Auto-refresh failed:', e);
      this.clearSession(true);
    }
  }

  signIn(promptMode = 'consent') {
    const clientId = this.config?.clientId;
    if (!clientId) {
      alert('Error de configuración: no se encontró CLIENT_ID de Google Drive en .env');
      return;
    }

    if (!window.google?.accounts?.oauth2) {
      alert('Google Identity Services aún no ha terminado de cargar en el navegador. Por favor espera unos segundos o recarga la página.');
      return;
    }

    // Siempre asegurar que el tokenClient esté creado con la configuración actual
    const scopes = 'https://www.googleapis.com/auth/drive https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile';
    this.tokenClient = window.google.accounts.oauth2.initTokenClient({
      client_id: clientId,
      scope: scopes,
      callback: (resp) => this.handleTokenResponse(resp),
    });

    try {
      const promptVal = promptMode === 'consent' ? 'consent' : '';
      console.log('[CloudDrive] Solicitando token de Google con prompt:', promptVal);
      this.tokenClient.requestAccessToken({ prompt: promptVal });
    } catch (e) {
      console.error('[CloudDrive] requestAccessToken falló, intentando sin opciones:', e);
      try {
        this.tokenClient.requestAccessToken();
      } catch (e2) {
        alert('Error al abrir la ventana de Google: ' + e2.message);
      }
    }
  }


  signOut() {
    if (this.authState.accessToken && window.google?.accounts?.oauth2) {
      try {
        window.google.accounts.oauth2.revoke(this.authState.accessToken, () => {});
      } catch {}
    }
    this.clearSession(true);
  }

  clearSession(notify = true) {
    this.authState.isAuthenticated = false;
    this.authState.accessToken = null;
    this.authState.user = null;
    localStorage.removeItem('google_auth');
    localStorage.removeItem('designcraft_cloud_auth');
    if (notify) {
      this.notify();
    }
  }

  subscribe(listener) {
    this.listeners.add(listener);
    listener(this.authState);
    return () => this.listeners.delete(listener);
  }

  notify() {
    for (const l of this.listeners) l(this.authState);
  }

  notifyListeners() {
    this.notify();
  }

  getAccessToken() {
    return this.authState.accessToken;
  }

  async getValidAccessToken() {
    const auth = this.getStoredAuth();
    if (!auth || !auth.accessToken) {
      this.clearSession(true);
      return null;
    }
    const now = Date.now();
    if (auth.expiresAt && auth.expiresAt > now + 60000 && this.authState.accessToken) {
      return this.authState.accessToken;
    }
    if (auth.refreshToken && this.config?.clientSecret) {
      await this.refreshAccessToken(auth.refreshToken);
      if (this.authState.accessToken) return this.authState.accessToken;
    }
    // Token expired and cannot be refreshed
    this.clearSession(true);
    return null;
  }

  // --- GOOGLE DRIVE API CALLS ---

  async driveRequest(url, options = {}) {
    const token = this.getAccessToken();
    if (!token) throw new Error('No hay sesión de Google activa.');
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), 12000);
    try {
      const res = await fetch(url, {
        ...options,
        signal: options.signal || controller.signal,
        headers: {
          ...options.headers,
          Authorization: `Bearer ${token}`,
        },
      });
      clearTimeout(timeoutId);
      if (!res.ok) {
        if (res.status === 401) {
          this.clearSession(true);
        }
        const err = await res.json().catch(() => ({ error: { message: res.statusText } }));
        throw new Error(err.error?.message || `Google Drive error ${res.status}: ${res.statusText}`);
      }
      return res;
    } catch (e) {
      clearTimeout(timeoutId);
      if (e.name === 'AbortError') {
        throw new Error('Tiempo de espera agotado al conectar con Google Drive');
      }
      throw e;
    }
  }

  // --- TEMPLATE GALLERY API ---

  // Lista carpetas de categorías de plantillas (o la raíz de plantillas)
  async listTemplateCategories() {
    const parentId = this.config?.templatesFolderId;
    if (!parentId) throw new Error('Falta VITE_DRIVE_TEMPLATES_FOLDER_ID en la configuración.');

    const query = `'${parentId}' in parents and mimeType='application/vnd.google-apps.folder' and trashed=false`;
    const url = `https://www.googleapis.com/drive/v3/files?q=${encodeURIComponent(query)}&orderBy=name asc&fields=files(id,name)&supportsAllDrives=true&includeItemsFromAllDrives=true`;
    
    const res = await this.driveRequest(url);
    const data = await res.json();
    return data.files || [];
  }

  // Lista plantillas emparejadas con sus thumbnails en una carpeta de Drive
  async listTemplatesInFolder(folderId) {
    const targetFolder = folderId || this.config?.templatesFolderId;
    if (!targetFolder) throw new Error('Falta carpeta de plantillas.');

    const query = `'${targetFolder}' in parents and trashed=false`;
    const url = `https://www.googleapis.com/drive/v3/files?q=${encodeURIComponent(query)}&fields=files(id,name,mimeType,thumbnailLink,size)&supportsAllDrives=true&includeItemsFromAllDrives=true`;

    const res = await this.driveRequest(url);
    const data = await res.json();
    const files = data.files || [];

    // Separar plantillas (idml / designcraft) e imágenes (jpg, png, webp)
    const docFiles = files.filter(f => f.name.toLowerCase().endsWith('.idml') || f.name.toLowerCase().endsWith('.designcraft'));
    const imageFiles = files.filter(f => {
      const n = f.name.toLowerCase();
      return n.endsWith('.jpg') || n.endsWith('.jpeg') || n.endsWith('.png') || n.endsWith('.webp');
    });

    const templates = [];
    for (const doc of docFiles) {
      const baseName = doc.name.replace(/\.(idml|designcraft)$/i, '');
      const baseLower = baseName.toLowerCase();

      // Buscar imagen con nombre coincidente
      const matchingImg = imageFiles.find(img => {
        const imgBase = img.name.replace(/\.(jpg|jpeg|png|webp)$/i, '').toLowerCase();
        return imgBase === baseLower || imgBase.startsWith(baseLower) || baseLower.startsWith(imgBase);
      });

      templates.push({
        id: doc.id,
        name: baseName.replace(/[_]/g, ' '),
        filename: doc.name,
        isIdml: doc.name.toLowerCase().endsWith('.idml'),
        isDesignCraft: doc.name.toLowerCase().endsWith('.designcraft'),
        size: Number(doc.size || 0),
        previewFileId: matchingImg ? matchingImg.id : null,
        previewFileName: matchingImg ? matchingImg.name : null,
        thumbnailLink: doc.thumbnailLink || null,
      });
    }

    templates.sort((a, b) => a.name.localeCompare(b.name));
    return templates;
  }

  // Descarga una imagen thumbnail con token de Google y devuelve URL blob utilizable en <img>
  async getThumbnailBlobUrl(fileId) {
    if (!fileId) return null;
    try {
      const url = `https://www.googleapis.com/drive/v3/files/${fileId}?alt=media&supportsAllDrives=true`;
      const res = await this.driveRequest(url);
      const blob = await res.blob();
      return URL.createObjectURL(blob);
    } catch (e) {
      console.warn(`[CloudDrive] Thumbnail download failed for ${fileId}:`, e);
      return null;
    }
  }

  // Lista de Ediciones (subcarpetas de destinationFolderId)
  async listEdiciones() {
    const parentId = this.config?.destinationFolderId;
    if (!parentId) throw new Error('Falta VITE_DRIVE_DESTINATION_FOLDER_ID en la configuración.');

    const query = `'${parentId}' in parents and mimeType='application/vnd.google-apps.folder' and trashed=false`;
    const url = `https://www.googleapis.com/drive/v3/files?q=${encodeURIComponent(query)}&orderBy=name desc&fields=files(id,name)&supportsAllDrives=true&includeItemsFromAllDrives=true`;
    
    const res = await this.driveRequest(url);
    const data = await res.json();
    return data.files || [];
  }

  // Lista de Páginas dentro de una Edición
  async listPages(edicionFolderId) {
    const query = `'${edicionFolderId}' in parents and mimeType='application/vnd.google-apps.folder' and trashed=false`;
    const url = `https://www.googleapis.com/drive/v3/files?q=${encodeURIComponent(query)}&orderBy=name asc&fields=files(id,name)&supportsAllDrives=true&includeItemsFromAllDrives=true`;
    
    const res = await this.driveRequest(url);
    const data = await res.json();
    return data.files || [];
  }

  // Lista de archivos dentro de una Página
  async listFiles(folderId) {
    const query = `'${folderId}' in parents and trashed=false`;
    const url = `https://www.googleapis.com/drive/v3/files?q=${encodeURIComponent(query)}&fields=files(id,name,mimeType,size,modifiedTime)&supportsAllDrives=true&includeItemsFromAllDrives=true`;
    
    const res = await this.driveRequest(url);
    const data = await res.json();
    return (data.files || []).map(f => ({
      id: f.id,
      name: f.name,
      mimeType: f.mimeType,
      size: Number(f.size || 0),
      modifiedTime: f.modifiedTime,
      isDesignCraft: f.name.endsWith('.designcraft'),
      isIdml: f.name.toLowerCase().endsWith('.idml'),
      isPdf: f.name.toLowerCase().endsWith('.pdf')
    }));
  }

  // Descarga bytes de un archivo en Drive
  async downloadFileBytes(fileId) {
    const url = `https://www.googleapis.com/drive/v3/files/${fileId}?alt=media&supportsAllDrives=true`;
    const res = await this.driveRequest(url);
    const blob = await res.blob();
    const buffer = await blob.arrayBuffer();
    return new Uint8Array(buffer);
  }

  // Subida de documento a la carpeta de la página
  async uploadFile(fileName, uint8Array, folderId, mimeType = 'application/octet-stream') {
    const token = this.getAccessToken();
    if (!token) throw new Error('No hay sesión de Google activa.');

    // Verificar si ya existe un archivo con ese nombre en esa carpeta para actualizarlo
    let existingId = null;
    try {
      const existingQuery = `'${folderId}' in parents and name='${fileName.replace(/'/g, "\\'")}' and trashed=false`;
      const searchUrl = `https://www.googleapis.com/drive/v3/files?q=${encodeURIComponent(existingQuery)}&fields=files(id,name)&supportsAllDrives=true&includeItemsFromAllDrives=true`;
      const searchRes = await this.driveRequest(searchUrl);
      const searchData = await searchRes.json();
      if (searchData.files && searchData.files.length > 0) {
        existingId = searchData.files[0].id;
      }
    } catch (e) {
      console.warn('[CloudDrive] Check existing file error:', e);
    }

    const blob = new Blob([uint8Array], { type: mimeType });

    if (existingId) {
      // Actualizar archivo existente (PATCH)
      const uploadUrl = `https://www.googleapis.com/upload/drive/v3/files/${existingId}?uploadType=media&supportsAllDrives=true`;
      const res = await fetch(uploadUrl, {
        method: 'PATCH',
        headers: {
          Authorization: `Bearer ${token}`,
          'Content-Type': mimeType,
        },
        body: blob,
      });
      if (!res.ok) throw new Error(`Error al actualizar archivo en Drive: ${res.statusText}`);
      const data = await res.json();
      return { id: data.id, name: fileName, updated: true };
    } else {
      // Crear nuevo archivo (multipart POST)
      const metadata = {
        name: fileName,
        parents: [folderId],
      };
      const form = new FormData();
      form.append('metadata', new Blob([JSON.stringify(metadata)], { type: 'application/json' }));
      form.append('file', blob);

      const uploadUrl = 'https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart&supportsAllDrives=true';
      const res = await fetch(uploadUrl, {
        method: 'POST',
        headers: {
          Authorization: `Bearer ${token}`,
        },
        body: form,
      });
      if (!res.ok) throw new Error(`Error al subir archivo a Drive: ${res.statusText}`);
      const data = await res.json();
      return { id: data.id, name: data.name, updated: false };
    }
  }
}

// Instanciar globalmente
window.DesignCraftCloud = new CloudDriveManager();
export const cloudDrive = window.DesignCraftCloud;
