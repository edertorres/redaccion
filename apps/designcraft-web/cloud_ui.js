/**
 * Cloud UI Modal & Workflow for DesignCraft Web (Light Theme / Interfaz Clara)
 * Implements the 2-step editorial cycle:
 * Step 1: Browse & Load Template from Drive (organized by Thematic Folders with Visual Thumbnails)
 * Step 2: Save Finished Layout & Press PDF into Daily Edition Folder in Drive
 */

export function showCloudToast({ type = 'success', title = 'Éxito', message = '', duration = 7000 } = {}) {
  let toastContainer = document.getElementById('designcraft-toast-container');
  if (!toastContainer) {
    toastContainer = document.createElement('div');
    toastContainer.id = 'designcraft-toast-container';
    toastContainer.style.cssText = `
      position: fixed;
      top: 24px;
      right: 24px;
      z-index: 1000000;
      display: flex;
      flex-direction: column;
      gap: 12px;
      pointer-events: none;
      max-width: 440px;
      width: calc(100vw - 48px);
      font-family: system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif;
    `;
    document.body.appendChild(toastContainer);
  }

  const toast = document.createElement('div');
  const isSuccess = type === 'success';
  const isError = type === 'error';
  const borderColor = isSuccess ? '#16a34a' : (isError ? '#dc2626' : '#2563eb');
  const iconSvg = isSuccess ? `
    <div style="width: 36px; height: 36px; border-radius: 50%; background: #dcfce7; display: flex; align-items: center; justify-content: center; flex-shrink: 0; box-shadow: 0 2px 6px rgba(22, 163, 74, 0.2);">
      <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="#16a34a" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
        <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"></path>
        <polyline points="22 4 12 14.01 9 11.01"></polyline>
      </svg>
    </div>
  ` : (isError ? `
    <div style="width: 36px; height: 36px; border-radius: 50%; background: #fee2e2; display: flex; align-items: center; justify-content: center; flex-shrink: 0; box-shadow: 0 2px 6px rgba(220, 38, 38, 0.2);">
      <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="#dc2626" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="12" cy="12" r="10"></circle>
        <line x1="15" y1="9" x2="9" y2="15"></line>
        <line x1="9" y1="9" x2="15" y2="15"></line>
      </svg>
    </div>
  ` : `
    <div style="width: 36px; height: 36px; border-radius: 50%; background: #dbeafe; display: flex; align-items: center; justify-content: center; flex-shrink: 0;">
      <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="#2563eb" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="12" cy="12" r="10"></circle>
        <line x1="12" y1="16" x2="12" y2="12"></line>
        <line x1="12" y1="8" x2="12.01" y2="8"></line>
      </svg>
    </div>
  `);

  toast.style.cssText = `
    pointer-events: auto;
    position: relative;
    background: #ffffff;
    color: #0f172a;
    border-radius: 12px;
    padding: 16px 18px;
    box-shadow: 0 12px 32px rgba(15, 23, 42, 0.16), 0 2px 6px rgba(0,0,0,0.06);
    border: 1px solid #e2e8f0;
    border-left: 5px solid ${borderColor};
    display: flex;
    align-items: flex-start;
    gap: 14px;
    transform: translateX(120%);
    opacity: 0;
    transition: transform 0.35s cubic-bezier(0.16, 1, 0.3, 1), opacity 0.3s ease;
    overflow: hidden;
  `;

  toast.innerHTML = `
    ${iconSvg}
    <div style="flex: 1; min-width: 0;">
      <div style="font-weight: 700; font-size: 14px; color: #0f172a; margin-bottom: 3px; display: flex; align-items: center; gap: 6px;">
        ${title}
      </div>
      <div style="font-size: 12.5px; color: #475569; line-height: 1.5; word-break: break-word;">
        ${message}
      </div>
    </div>
    <button class="toast-close-btn" style="background: none; border: none; font-size: 22px; line-height: 1; color: #94a3b8; cursor: pointer; padding: 0 0 0 8px; margin: -4px -4px 0 0; transition: color 0.15s ease;" title="Cerrar">&times;</button>
    <div class="toast-progress-bar" style="position: absolute; bottom: 0; left: 0; height: 3px; background: ${borderColor}; width: 100%; transition: width ${duration}ms linear;"></div>
  `;

  toastContainer.appendChild(toast);

  requestAnimationFrame(() => {
    toast.style.transform = 'translateX(0)';
    toast.style.opacity = '1';
    const bar = toast.querySelector('.toast-progress-bar');
    if (bar) {
      requestAnimationFrame(() => {
        bar.style.width = '0%';
      });
    }
  });

  const closeToast = () => {
    toast.style.transform = 'translateX(120%)';
    toast.style.opacity = '0';
    setTimeout(() => {
      toast.remove();
    }, 350);
  };

  toast.querySelector('.toast-close-btn')?.addEventListener('click', closeToast);
  const timeoutId = setTimeout(closeToast, duration);
  toast.addEventListener('mouseenter', () => clearTimeout(timeoutId));
}

function escapeHtml(str) {
  if (!str) return '';
  return String(str)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

export function showGoogleAuthModal({
  title = 'Google Drive Desconectado',
  message = 'Tu sesión de Google Drive ha expirado o no ha sido iniciada. Para descargar maquetas y abrir plantillas de la nube, debes conectar tu cuenta de Google.',
  onLogin,
} = {}) {
  const existing = document.getElementById('google-auth-alert-overlay');
  if (existing) existing.remove();

  const overlay = document.createElement('div');
  overlay.id = 'google-auth-alert-overlay';
  overlay.style.cssText = `
    position: fixed;
    inset: 0;
    background: rgba(15, 23, 42, 0.72);
    backdrop-filter: blur(5px);
    z-index: 1000005;
    display: flex;
    align-items: center;
    justify-content: center;
    font-family: system-ui, -apple-system, sans-serif;
    padding: 16px;
    box-sizing: border-box;
  `;

  overlay.innerHTML = `
    <div style="background: #ffffff; border-radius: 16px; width: 480px; max-width: 100%; box-shadow: 0 25px 50px -12px rgba(220, 38, 38, 0.35); overflow: hidden; border: 2px solid #ef4444; animation: cloudModalIn 0.2s ease-out;">
      <div style="background: #fef2f2; padding: 20px 24px; border-bottom: 1px solid #fee2e2; display: flex; align-items: center; justify-content: space-between; gap: 14px;">
        <div style="display: flex; align-items: center; gap: 12px;">
          <div style="width: 42px; height: 42px; border-radius: 50%; background: #fee2e2; display: flex; align-items: center; justify-content: center; font-size: 22px; border: 1.5px solid #fca5a5; flex-shrink: 0;">
            ⚠️
          </div>
          <div>
            <h3 style="margin: 0; font-size: 16px; font-weight: 800; color: #991b1b;">${escapeHtml(title)}</h3>
            <p style="margin: 3px 0 0 0; font-size: 12.5px; color: #b91c1c;">Autorización de Google Drive requerida</p>
          </div>
        </div>
        <button id="btn-auth-alert-close-x" style="background: none; border: none; font-size: 22px; color: #94a3b8; cursor: pointer; padding: 4px; border-radius: 6px;">✕</button>
      </div>

      <div style="padding: 24px; color: #334155; font-size: 14px; line-height: 1.6;">
        <p style="margin: 0 0 16px 0; font-weight: 500;">${escapeHtml(message)}</p>
        <div style="background: #f8fafc; border: 1px solid #e2e8f0; border-radius: 10px; padding: 12px 14px; font-size: 12.5px; color: #64748b; display: flex; align-items: center; gap: 10px;">
          <span style="font-size: 20px;">💡</span>
          <span>Al conectar tu cuenta de Google, el servidor editorial podrá sincronizar y descargar tus plantillas directamente a tu entorno de maquetación.</span>
        </div>
      </div>

      <div style="padding: 16px 24px; background: #f8fafc; border-top: 1px solid #e2e8f0; display: flex; justify-content: flex-end; gap: 10px;">
        <button id="btn-auth-alert-cancel" type="button" style="background: #ffffff; color: #475569; border: 1px solid #cbd5e1; border-radius: 8px; padding: 10px 18px; font-size: 13.5px; font-weight: 600; cursor: pointer;">
          Cerrar
        </button>
        <button id="btn-auth-alert-login" type="button" style="background: #2563eb; color: #ffffff; border: none; border-radius: 8px; padding: 10px 22px; font-size: 13.5px; font-weight: 700; cursor: pointer; box-shadow: 0 4px 14px rgba(37, 99, 235, 0.35); display: flex; align-items: center; gap: 8px;">
          🔑 Iniciar Sesión con Google
        </button>
      </div>
    </div>
  `;

  document.body.appendChild(overlay);

  const closeAlert = () => overlay.remove();
  overlay.querySelector('#btn-auth-alert-cancel')?.addEventListener('click', closeAlert);
  overlay.querySelector('#btn-auth-alert-close-x')?.addEventListener('click', closeAlert);
  overlay.querySelector('#btn-auth-alert-login')?.addEventListener('click', () => {
    overlay.remove();
    if (onLogin) {
      onLogin();
    } else {
      window.DesignCraftCloud?.signIn();
    }
  });
}

if (typeof window !== 'undefined') {
  window.showCloudToast = showCloudToast;
  window.showGoogleAuthModal = showGoogleAuthModal;
}

export function initCloudUI() {
  const container = document.createElement('div');
  container.id = 'designcraft-cloud-ui';
  container.innerHTML = `
    <style>
      #cloud-pill-btn {
        position: fixed;
        top: 10px;
        right: 14px;
        z-index: 9999;
        display: flex;
        align-items: center;
        gap: 8px;
        background: #ffffff;
        color: #1e293b;
        border: 1px solid #cbd5e1;
        padding: 6px 14px;
        border-radius: 20px;
        font-family: system-ui, -apple-system, sans-serif;
        font-size: 13px;
        font-weight: 500;
        cursor: pointer;
        box-shadow: 0 4px 12px rgba(0,0,0,0.08);
        transition: all 0.2s ease;
      }
      #cloud-pill-btn:hover {
        background: #f8fafc;
        border-color: #3b82f6;
        color: #1d4ed8;
      }
      #cloud-pill-btn img {
        width: 22px;
        height: 22px;
        border-radius: 50%;
      }
      #cloud-modal-overlay {
        display: none;
        position: fixed;
        inset: 0;
        background: rgba(15, 23, 42, 0.45);
        backdrop-filter: blur(4px);
        z-index: 10000;
        align-items: center;
        justify-content: center;
      }
      #cloud-modal-overlay.open {
        display: flex;
      }
      .cloud-modal {
        background: #ffffff;
        color: #0f172a;
        border: 1px solid #e2e8f0;
        border-radius: 16px;
        width: 1280px;
        max-width: 96vw;
        height: 90vh;
        max-height: 92vh;
        box-shadow: 0 25px 60px -15px rgba(15, 23, 42, 0.25), 0 0 0 1px rgba(0, 0, 0, 0.05);
        font-family: system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif;
        display: flex;
        flex-direction: column;
        overflow: hidden;
        animation: cloudModalIn 0.22s cubic-bezier(0.16, 1, 0.3, 1);
      }
      @keyframes cloudModalIn {
        from { transform: scale(0.97); opacity: 0; }
        to { transform: scale(1); opacity: 1; }
      }
      .cloud-modal-header {
        padding: 20px 28px;
        border-bottom: 1px solid #f1f5f9;
        display: flex;
        justify-content: space-between;
        align-items: center;
        background: #ffffff;
      }
      .cloud-modal-header-left h2 {
        margin: 0;
        font-size: 22px;
        font-weight: 700;
        color: #111827;
        letter-spacing: -0.3px;
      }
      .cloud-modal-close {
        background: none;
        border: none;
        color: #6b7280;
        font-size: 22px;
        cursor: pointer;
        padding: 6px 10px;
        border-radius: 8px;
        line-height: 1;
        transition: all 0.15s ease;
      }
      .cloud-modal-close:hover {
        color: #111827;
        background: #f3f4f6;
      }

      /* Clean Template Gallery (IDML Injector Pro style) */
      .clean-template-card {
        border: 1px solid #e5e7eb;
        border-radius: 10px;
        overflow: hidden;
        background: #ffffff;
        cursor: pointer;
        transition: box-shadow 0.2s ease, border-color 0.2s ease, transform 0.2s ease;
        display: flex;
        flex-direction: column;
        position: relative;
        box-shadow: 0 1px 3px rgba(0,0,0,0.04);
      }
      .clean-template-card:hover {
        border-color: #cbd5e1;
        box-shadow: 0 10px 25px -5px rgba(0, 0, 0, 0.1), 0 8px 10px -6px rgba(0, 0, 0, 0.05);
      }
      .clean-card-thumb-container {
        height: 256px;
        background: #f3f4f6;
        position: relative;
        display: flex;
        align-items: center;
        justify-content: center;
        overflow: hidden;
      }
      .clean-card-img {
        max-width: 100%;
        max-height: 100%;
        object-fit: contain;
        transition: transform 0.25s ease;
      }
      .clean-template-card:hover .clean-card-img {
        transform: scale(1.02);
      }
      .clean-zoom-btn {
        position: absolute;
        top: 8px;
        right: 8px;
        padding: 6px 10px;
        background: rgba(255, 255, 255, 0.9);
        border: 1px solid rgba(0, 0, 0, 0.06);
        border-radius: 8px;
        box-shadow: 0 2px 6px rgba(0, 0, 0, 0.1);
        color: #4f46e5;
        font-size: 16px;
        cursor: pointer;
        opacity: 0;
        transition: opacity 0.15s ease, background 0.15s ease, transform 0.15s ease;
        z-index: 20;
      }
      .clean-template-card:hover .clean-zoom-btn {
        opacity: 1;
      }
      .clean-zoom-btn:hover {
        background: #ffffff;
        transform: scale(1.08);
      }
      .clean-cat-pill {
        padding: 8px 18px;
        border-radius: 8px;
        font-size: 13px;
        font-weight: 600;
        cursor: pointer;
        border: none;
        transition: all 0.15s ease;
        letter-spacing: 0.3px;
        text-transform: uppercase;
        user-select: none;
      }
      .clean-cat-pill:hover {
        transform: translateY(-1px);
        filter: brightness(0.96);
      }
      .clean-search-input:focus {
        border-color: #6366f1 !important;
        box-shadow: 0 0 0 3px rgba(99, 102, 241, 0.16) !important;
      }

      /* Tabs */
      .cloud-tabs {
        display: flex;
        background: #f1f5f9;
        border-bottom: 1px solid #e2e8f0;
        padding: 0 24px;
        gap: 6px;
      }
      .cloud-tab-btn {
        background: none;
        border: none;
        border-bottom: 3px solid transparent;
        color: #64748b;
        padding: 13px 20px;
        font-size: 13.5px;
        font-weight: 600;
        cursor: pointer;
        transition: all 0.15s ease;
        display: flex;
        align-items: center;
        gap: 8px;
      }
      .cloud-tab-btn:hover {
        color: #0f172a;
      }
      .cloud-tab-btn.active {
        color: #2563eb;
        background: #ffffff;
        border-bottom-color: #2563eb;
      }

      .cloud-modal-body {
        padding: 22px 28px;
        overflow-y: auto;
        flex: 1;
        display: flex;
        flex-direction: column;
        gap: 16px;
        background: #ffffff;
      }

      /* Top Controls Bar */
      .cloud-top-controls {
        display: flex;
        gap: 14px;
        align-items: center;
        justify-content: space-between;
        flex-wrap: wrap;
      }
      .cloud-search-wrapper {
        position: relative;
        flex: 1;
        min-width: 280px;
      }
      .cloud-search-input {
        width: 100%;
        box-sizing: border-box;
        padding: 10px 14px 10px 38px;
        background: #f8fafc;
        color: #0f172a;
        border: 1px solid #cbd5e1;
        border-radius: 10px;
        font-size: 13.5px;
        outline: none;
        transition: all 0.18s ease;
      }
      .cloud-search-input:focus {
        background: #ffffff;
        border-color: #2563eb;
        box-shadow: 0 0 0 3px rgba(37, 99, 235, 0.14);
      }
      .cloud-search-icon {
        position: absolute;
        left: 12px;
        top: 50%;
        transform: translateY(-50%);
        color: #94a3b8;
        pointer-events: none;
        font-size: 15px;
      }

      /* Filter Pills */
      .cloud-filter-pills {
        display: flex;
        gap: 6px;
        align-items: center;
      }
      .cloud-filter-pill {
        background: #f1f5f9;
        color: #475569;
        border: 1px solid #e2e8f0;
        padding: 7px 14px;
        border-radius: 20px;
        font-size: 12.5px;
        font-weight: 600;
        cursor: pointer;
        white-space: nowrap;
        display: flex;
        align-items: center;
        gap: 6px;
        transition: all 0.15s ease;
      }
      .cloud-filter-pill:hover {
        background: #e2e8f0;
        color: #0f172a;
      }
      .cloud-filter-pill.active {
        background: #eff6ff;
        color: #1d4ed8;
        border-color: #3b82f6;
        box-shadow: 0 1px 3px rgba(37, 99, 235, 0.18);
      }

      /* Breadcrumb Navigation */
      .cloud-breadcrumb-nav {
        display: flex;
        align-items: center;
        justify-content: space-between;
        background: #f8fafc;
        border: 1px solid #e2e8f0;
        border-radius: 10px;
        padding: 10px 16px;
        gap: 12px;
        flex-wrap: wrap;
      }
      .cloud-breadcrumb-left {
        display: flex;
        align-items: center;
        gap: 12px;
      }
      .cloud-back-btn {
        background: #ffffff;
        border: 1px solid #cbd5e1;
        color: #334155;
        padding: 7px 14px;
        border-radius: 8px;
        font-size: 13px;
        font-weight: 600;
        cursor: pointer;
        display: flex;
        align-items: center;
        gap: 6px;
        transition: all 0.15s ease;
      }
      .cloud-back-btn:hover {
        background: #f1f5f9;
        border-color: #94a3b8;
        color: #0f172a;
      }
      .cloud-breadcrumb-title {
        font-size: 15px;
        font-weight: 700;
        color: #0f172a;
        display: flex;
        align-items: center;
        gap: 8px;
      }
      .cloud-breadcrumb-count {
        font-size: 12.5px;
        color: #64748b;
        font-weight: 500;
      }

      /* Folder Cards Grid (Filas y Columnas) */
      .cloud-folders-grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(285px, 1fr));
        gap: 20px;
        padding-bottom: 24px;
      }
      .cloud-folder-card {
        background: #ffffff;
        border: 1.5px solid #e2e8f0;
        border-radius: 14px;
        overflow: hidden;
        cursor: pointer;
        transition: all 0.22s cubic-bezier(0.16, 1, 0.3, 1);
        display: flex;
        flex-direction: column;
        position: relative;
        box-shadow: 0 2px 6px rgba(0, 0, 0, 0.04);
      }
      .cloud-folder-card:hover {
        transform: translateY(-4px);
        box-shadow: 0 16px 32px -8px rgba(15, 23, 42, 0.12), 0 4px 12px rgba(37, 99, 235, 0.08);
        border-color: #93c5fd;
      }
      .cloud-folder-accent-bar {
        height: 5px;
        width: 100%;
      }
      .cloud-folder-body {
        padding: 18px 20px;
        display: flex;
        flex-direction: column;
        flex: 1;
      }
      .cloud-folder-top-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        margin-bottom: 12px;
      }
      .cloud-folder-icon-circle {
        width: 42px;
        height: 42px;
        border-radius: 10px;
        display: flex;
        align-items: center;
        justify-content: center;
        font-size: 21px;
        box-shadow: 0 2px 6px rgba(0,0,0,0.06);
      }
      .cloud-folder-count-badge {
        font-size: 12px;
        font-weight: 600;
        padding: 4px 10px;
        border-radius: 14px;
        background: #f1f5f9;
        color: #334155;
        border: 1px solid #e2e8f0;
      }
      .cloud-folder-title {
        font-size: 16px;
        font-weight: 700;
        color: #0f172a;
        margin: 0 0 5px 0;
      }
      .cloud-folder-desc {
        font-size: 12px;
        color: #64748b;
        line-height: 1.45;
        margin: 0 0 14px 0;
        min-height: 35px;
      }
      .cloud-folder-preview-strip {
        display: flex;
        gap: 8px;
        height: 84px;
        background: #f8fafc;
        border-radius: 8px;
        border: 1px solid #f1f5f9;
        padding: 6px 10px;
        align-items: center;
        overflow: hidden;
        margin-bottom: 14px;
      }
      .cloud-folder-mini-thumb {
        width: 58px;
        height: 72px;
        background: #ffffff;
        border: 1px solid #e2e8f0;
        border-radius: 5px;
        box-shadow: 0 2px 4px rgba(0,0,0,0.06);
        display: flex;
        align-items: center;
        justify-content: center;
        overflow: hidden;
        flex-shrink: 0;
        transition: transform 0.2s ease;
      }
      .cloud-folder-card:hover .cloud-folder-mini-thumb {
        transform: scale(1.05);
      }
      .cloud-folder-mini-img {
        width: 100%;
        height: 100%;
        object-fit: cover;
      }
      .cloud-folder-mini-placeholder {
        font-size: 20px;
        color: #94a3b8;
      }
      .cloud-folder-footer {
        margin-top: auto;
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding-top: 10px;
        border-top: 1px solid #f1f5f9;
      }
      .cloud-folder-formats {
        font-size: 11.5px;
        color: #64748b;
        font-weight: 500;
      }
      .cloud-folder-explore-btn {
        font-size: 12.5px;
        font-weight: 600;
        color: #2563eb;
        display: flex;
        align-items: center;
        gap: 4px;
        transition: transform 0.15s ease;
      }
      .cloud-folder-card:hover .cloud-folder-explore-btn {
        transform: translateX(3px);
        color: #1d4ed8;
      }

      /* Badges */
      .cloud-card-header-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        margin-bottom: 6px;
      }
      .cloud-cat-badge {
        font-size: 11px;
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.5px;
        color: #1d4ed8;
        background: #eff6ff;
        border: 1px solid #bfdbfe;
        padding: 2px 8px;
        border-radius: 4px;
        max-width: 140px;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
      }
      .cloud-type-badge {
        font-size: 10.5px;
        font-weight: 600;
        padding: 2px 7px;
        border-radius: 4px;
      }
      .type-idml {
        background: #fee2e2;
        color: #991b1b;
        border: 1px solid #fecaca;
      }
      .type-dc {
        background: #dcfce7;
        color: #166534;
        border: 1px solid #bbf7d0;
      }

      /* Template Grid */
      .cloud-grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(250px, 1fr));
        gap: 18px;
      }
      .cloud-card {
        background: #ffffff;
        border: 1px solid #e2e8f0;
        border-radius: 12px;
        overflow: hidden;
        cursor: pointer;
        transition: all 0.2s ease;
        display: flex;
        flex-direction: column;
        box-shadow: 0 1px 4px rgba(0,0,0,0.05);
      }
      .cloud-card:hover {
        border-color: #2563eb;
        transform: translateY(-3px);
        box-shadow: 0 12px 24px -6px rgba(37, 99, 235, 0.16);
      }
      .cloud-thumb-box {
        height: 200px;
        background: #f8fafc;
        border-bottom: 1px solid #f1f5f9;
        display: flex;
        align-items: center;
        justify-content: center;
        overflow: hidden;
        position: relative;
      }
      .cloud-thumb-img {
        max-width: 100%;
        max-height: 100%;
        object-fit: contain;
        transition: transform 0.25s ease;
      }
      .cloud-card:hover .cloud-thumb-img {
        transform: scale(1.03);
      }
      .cloud-thumb-placeholder {
        font-size: 46px;
        color: #94a3b8;
      }
      .cloud-card-body {
        padding: 14px 16px;
        display: flex;
        flex-direction: column;
        flex: 1;
      }
      .cloud-card-title {
        font-size: 14px;
        font-weight: 600;
        margin: 0 0 4px 0;
        color: #0f172a;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
      }
      .cloud-card-meta {
        font-size: 12px;
        color: #64748b;
        margin: 0 0 12px 0;
      }

      /* Fields & Forms */
      .cloud-field label {
        display: block;
        font-size: 12px;
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.5px;
        color: #475569;
        margin-bottom: 6px;
      }
      .cloud-select {
        width: 100%;
        padding: 10px 14px;
        background: #ffffff;
        color: #0f172a;
        border: 1px solid #cbd5e1;
        border-radius: 8px;
        font-size: 14px;
        outline: none;
        box-sizing: border-box;
      }
      .cloud-select:focus {
        border-color: #2563eb;
        box-shadow: 0 0 0 3px rgba(37, 99, 235, 0.12);
      }
      .cloud-btn {
        display: flex;
        align-items: center;
        justify-content: center;
        gap: 8px;
        padding: 10px 18px;
        border-radius: 8px;
        font-size: 13.5px;
        font-weight: 600;
        cursor: pointer;
        border: none;
        transition: all 0.15s ease;
      }
      .cloud-btn-primary {
        background: #2563eb;
        color: #ffffff;
      }
      .cloud-btn-primary:hover {
        background: #1d4ed8;
      }
      .cloud-btn-success {
        background: #16a34a;
        color: #ffffff;
      }
      .cloud-btn-success:hover {
        background: #15803d;
      }
      .cloud-btn-ghost {
        background: #f1f5f9;
        color: #334155;
        border: 1px solid #cbd5e1;
      }
      .cloud-btn-ghost:hover {
        background: #e2e8f0;
        color: #0f172a;
      }
      .cloud-user-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        background: #f8fafc;
        border: 1px solid #e2e8f0;
        padding: 10px 14px;
        border-radius: 8px;
      }
      .cloud-user-info {
        display: flex;
        align-items: center;
        gap: 10px;
      }
      .cloud-user-info img {
        width: 32px;
        height: 32px;
        border-radius: 50%;
      }
      .cloud-status-badge {
        font-size: 13px;
        padding: 10px 14px;
        border-radius: 8px;
        background: #eff6ff;
        color: #1d4ed8;
        border: 1px solid #bfdbfe;
      }

      /* Floating High-Resolution Enlarged Hover Preview Popover */
      #cloud-thumb-hover-preview {
        position: fixed;
        z-index: 1000005;
        width: 350px;
        max-width: 90vw;
        background: #ffffff;
        border-radius: 16px;
        border: 1px solid #cbd5e1;
        box-shadow: 0 24px 60px -8px rgba(15, 23, 42, 0.35), 0 4px 16px rgba(0, 0, 0, 0.08);
        display: flex;
        flex-direction: column;
        overflow: hidden;
        opacity: 0;
        visibility: hidden;
        pointer-events: none;
        transform: scale(0.94);
        transition: opacity 0.16s ease, transform 0.16s cubic-bezier(0.16, 1, 0.3, 1), visibility 0.16s;
      }
      #cloud-thumb-hover-preview.active {
        opacity: 1;
        visibility: visible;
        transform: scale(1);
      }
      .cloud-hover-popover-header {
        padding: 12px 16px 10px 16px;
        background: #f8fafc;
        border-bottom: 1px solid #e2e8f0;
        display: flex;
        flex-direction: column;
        gap: 6px;
      }
      .cloud-hover-tags {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 8px;
      }
      .cloud-hover-sec-tag {
        font-size: 11px;
        font-weight: 700;
        text-transform: uppercase;
        letter-spacing: 0.5px;
        padding: 3px 8px;
        border-radius: 4px;
        color: #1e40af;
        background: #dbeafe;
      }
      .cloud-hover-fmt-tag {
        font-size: 11px;
        font-weight: 700;
        padding: 3px 8px;
        border-radius: 4px;
      }
      .cloud-hover-title {
        font-size: 14px;
        font-weight: 700;
        color: #0f172a;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
      }
      .cloud-hover-preview-body {
        padding: 14px;
        background: #f1f5f9;
        display: flex;
        align-items: center;
        justify-content: center;
        min-height: 400px;
        max-height: 460px;
      }
      .cloud-hover-page-frame {
        width: 100%;
        height: 100%;
        max-height: 430px;
        aspect-ratio: 240 / 330;
        background: #ffffff;
        border-radius: 6px;
        box-shadow: 0 4px 16px rgba(15, 23, 42, 0.14), 0 1px 3px rgba(0,0,0,0.06);
        border: 1px solid #cbd5e1;
        overflow: hidden;
        display: flex;
        align-items: center;
        justify-content: center;
        position: relative;
      }
      .cloud-hover-large-img {
        width: 100%;
        height: 100%;
        object-fit: contain;
        display: block;
      }
      .cloud-hover-popover-footer {
        padding: 10px 16px;
        background: #ffffff;
        border-top: 1px solid #f1f5f9;
        display: flex;
        align-items: center;
        justify-content: space-between;
        font-size: 12px;
        color: #64748b;
      }
      .cloud-hover-footer-hint {
        color: #2563eb;
        font-weight: 600;
        display: flex;
        align-items: center;
        gap: 4px;
      }

      /* Zoom Badge on Cards */
      .cloud-thumb-zoom-badge {
        position: absolute;
        top: 8px;
        right: 8px;
        width: 28px;
        height: 28px;
        border-radius: 50%;
        background: rgba(255, 255, 255, 0.94);
        backdrop-filter: blur(4px);
        border: 1px solid rgba(203, 213, 225, 0.85);
        display: flex;
        align-items: center;
        justify-content: center;
        font-size: 13px;
        color: #1e293b;
        opacity: 0;
        transform: scale(0.8);
        transition: all 0.2s cubic-bezier(0.16, 1, 0.3, 1);
        box-shadow: 0 2px 8px rgba(0, 0, 0, 0.12);
        pointer-events: none;
        z-index: 2;
      }
      .cloud-card:hover .cloud-thumb-zoom-badge {
        opacity: 1;
        transform: scale(1);
      }
      .cloud-thumb-box {
        cursor: zoom-in;
      }
      .cloud-folder-mini-thumb {
        cursor: zoom-in;
      }
    </style>

    <button id="cloud-pill-btn" title="Nube Editorial Google Drive">
      <span id="cloud-pill-icon">☁</span>
      <span id="cloud-pill-text">Google Drive</span>
    </button>

    <div id="cloud-modal-overlay">
      <div class="cloud-modal">
        <div class="cloud-modal-header">
          <div class="cloud-modal-header-left">
            <h2 id="cloud-modal-header-title">Plantillas de Google Drive</h2>
          </div>
          <button class="cloud-modal-close" id="cloud-modal-close-btn">&times;</button>
        </div>

        <div class="cloud-tabs" id="cloud-tabs-bar" style="display: none;">
          <button class="cloud-tab-btn active" data-tab="templates">📂 1. Elegir Plantilla (Maqueta)</button>
          <button class="cloud-tab-btn" data-tab="save">💾 2. Guardar en Edición del Día</button>
        </div>

        <div class="cloud-modal-body" id="cloud-modal-content">
          <div style="text-align: center; padding: 24px; color: #64748b;">Cargando...</div>
        </div>
      </div>
    </div>

    <!-- Floating High-Resolution Enlarged Hover Preview Popover -->
    <div id="cloud-thumb-hover-preview" class="cloud-hover-popover">
      <div class="cloud-hover-popover-header">
        <div class="cloud-hover-tags">
          <span class="cloud-hover-sec-tag" id="chp-sec-tag">📰 PORTADA</span>
          <span class="cloud-hover-fmt-tag" id="chp-fmt-tag">⚡ DesignCraft</span>
        </div>
        <div class="cloud-hover-title" id="chp-title">Portada Principal</div>
      </div>
      <div class="cloud-hover-preview-body">
        <div class="cloud-hover-page-frame" id="chp-page-frame"></div>
      </div>
      <div class="cloud-hover-popover-footer">
        <div class="cloud-hover-footer-meta" id="chp-meta">Plantilla editorial</div>
        <div class="cloud-hover-footer-hint">👆 Clic para maquetar</div>
      </div>
    </div>
  `;

  document.body.appendChild(container);

  const pillBtn = document.getElementById('cloud-pill-btn');
  const overlay = document.getElementById('cloud-modal-overlay');
  const closeBtn = document.getElementById('cloud-modal-close-btn');
  const modalContent = document.getElementById('cloud-modal-content');
  const tabsBar = document.getElementById('cloud-tabs-bar');

  let currentTab = 'templates'; // 'templates' | 'save'
  let cachedTemplates = [];
  let cachedCategories = [];
  let activeFolderId = null; // null = Grid de Carpetas, string = Detalle de carpeta
  let activeFormatFilter = 'all'; // 'all' | 'dc' | 'idml'
  let activeSearchQuery = '';
  const thumbnailBlobCache = new Map();
  let cachedEdiciones = [];
  let cachedPages = [];
  let selectedEdicionId = '';
  let selectedPageId = '';

  function getCategoryMeta(catId, catName) {
    const rawName = catName || catId || 'General';
    return {
      name: rawName,
      icon: '📁',
      accent: 'linear-gradient(90deg, #3b82f6, #1d4ed8)',
      bgTint: '#eff6ff',
      desc: '',
      defaultPage: 1,
      standardSection: rawName.toUpperCase().replace(/[^A-Z0-9_]/g, '_')
    };
  }

  function escapeHtml(str) {
    if (!str) return '';
    return String(str)
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;');
  }

  function renderEditorialSvgWireframe(tmpl, meta, mode = 'card') {
    const secColor = '#2563eb';
    const folderName = (tmpl?.category || meta?.name || 'General').trim();
    const secName = folderName.toUpperCase();
    const isIdml = tmpl?.isIdml;
    const rawName = tmpl?.name || 'Plantilla';
    const name = escapeHtml(rawName.length > 20 ? rawName.slice(0, 18) + '...' : rawName);

    if (mode === true || mode === 'mini') {
      return `
        <svg viewBox="0 0 44 58" xmlns="http://www.w3.org/2000/svg" style="width:100%;height:100%;background:#ffffff;display:block;">
          <rect width="44" height="58" fill="#ffffff" />
          <rect x="0.5" y="0.5" width="43" height="57" fill="none" stroke="#e2e8f0" stroke-width="0.8" />
          <rect x="2" y="2" width="40" height="9" fill="${secColor}" opacity="0.95" rx="1" />
          <text x="4" y="8.5" font-family="system-ui, -apple-system, sans-serif" font-size="5" font-weight="800" fill="#ffffff">${escapeHtml(secName.slice(0, 9))}</text>
          <rect x="2" y="13" width="40" height="4" fill="#0f172a" rx="0.5" />
          <text x="4" y="16.2" font-family="system-ui, -apple-system, sans-serif" font-size="3.5" font-weight="700" fill="#ffffff">${escapeHtml(name.slice(0, 11))}</text>
          <rect x="2" y="19" width="40" height="18" fill="#f1f5f9" stroke="#cbd5e1" stroke-width="0.5" rx="1" />
          <g transform="translate(18, 24)" opacity="0.4">
            <circle cx="4" cy="4" r="3.5" fill="#94a3b8" />
          </g>
          <rect x="2" y="39" width="11" height="13" fill="#f8fafc" stroke="#e2e8f0" stroke-width="0.4" rx="0.5" />
          <rect x="16.5" y="39" width="11" height="13" fill="#f8fafc" stroke="#e2e8f0" stroke-width="0.4" rx="0.5" />
          <rect x="31" y="39" width="11" height="13" fill="#f8fafc" stroke="#e2e8f0" stroke-width="0.4" rx="0.5" />
          <line x1="2" y1="54" x2="42" y2="54" stroke="#e2e8f0" stroke-width="0.5" />
        </svg>
      `;
    }

    if (mode === 'large') {
      const displayTitle = rawName.replace(/[\-_]/g, ' ');
      return `
        <svg viewBox="0 0 240 330" xmlns="http://www.w3.org/2000/svg" style="width:100%;height:100%;background:#ffffff;display:block;margin:auto;">
          <!-- Sheet -->
          <rect width="240" height="330" fill="#ffffff" />
          <rect x="0.5" y="0.5" width="239" height="329" fill="none" stroke="#cbd5e1" stroke-width="1" />
          
          <!-- Outer Margins -->
          <rect x="14" y="14" width="212" height="302" fill="none" stroke="#f1f5f9" stroke-width="0.8" stroke-dasharray="3 2" />
          
          <!-- Top Section Header with Folder Name -->
          <rect x="14" y="14" width="212" height="20" fill="${secColor}" opacity="0.95" rx="2" />
          <text x="22" y="27.5" font-family="system-ui, -apple-system, sans-serif" font-size="10" font-weight="800" fill="#ffffff" letter-spacing="0.8">${escapeHtml(secName)}</text>
          
          <!-- Main Headline with Template Name -->
          <rect x="14" y="40" width="212" height="16" fill="#0f172a" rx="2" />
          <text x="22" y="52" font-family="system-ui, -apple-system, sans-serif" font-size="9.5" font-weight="700" fill="#ffffff">${escapeHtml(displayTitle.length > 28 ? displayTitle.slice(0, 26) + '...' : displayTitle)}</text>

          <!-- Main Photo Container -->
          <defs>
            <linearGradient id="photoGradLarge" x1="0%" y1="0%" x2="100%" y2="100%">
              <stop offset="0%" stop-color="#f8fafc" />
              <stop offset="100%" stop-color="#e2e8f0" />
            </linearGradient>
          </defs>
          <rect x="14" y="64" width="212" height="121" fill="url(#photoGradLarge)" stroke="#cbd5e1" stroke-width="1" rx="3" />
          
          <!-- Camera Focal Point -->
          <g transform="translate(100, 110)" opacity="0.55">
            <rect x="-10" y="-8" width="60" height="38" fill="#cbd5e1" rx="4" />
            <circle cx="20" cy="11" r="11" fill="#94a3b8" />
            <path d="M6 25l8-8 6 6 8-10 10 12H6z" fill="#ffffff" />
          </g>
          
          <rect x="14" y="188" width="212" height="4" fill="#f1f5f9" rx="1" />
          
          <!-- 3-Column Text Body -->
          <g transform="translate(14, 202)">
            <rect x="0" y="0" width="65" height="5" fill="#475569" opacity="0.8" rx="1" />
            <rect x="0" y="8" width="65" height="3" fill="#cbd5e1" rx="0.5" />
            <rect x="0" y="14" width="65" height="3" fill="#cbd5e1" rx="0.5" />
            <rect x="0" y="20" width="65" height="3" fill="#cbd5e1" rx="0.5" />
            <rect x="0" y="26" width="65" height="3" fill="#cbd5e1" rx="0.5" />
            <rect x="0" y="32" width="65" height="3" fill="#cbd5e1" rx="0.5" />
            <rect x="0" y="38" width="50" height="3" fill="#cbd5e1" rx="0.5" />
          </g>
          
          <line x1="84.5" y1="202" x2="84.5" y2="246" stroke="#e2e8f0" stroke-width="0.8" />
          
          <g transform="translate(90, 202)">
            <rect x="0" y="0" width="65" height="5" fill="#475569" opacity="0.8" rx="1" />
            <rect x="0" y="8" width="65" height="3" fill="#cbd5e1" rx="0.5" />
            <rect x="0" y="14" width="65" height="3" fill="#cbd5e1" rx="0.5" />
            <rect x="0" y="20" width="65" height="3" fill="#cbd5e1" rx="0.5" />
            <rect x="0" y="26" width="65" height="3" fill="#cbd5e1" rx="0.5" />
            <rect x="0" y="32" width="65" height="3" fill="#cbd5e1" rx="0.5" />
            <rect x="0" y="38" width="58" height="3" fill="#cbd5e1" rx="0.5" />
          </g>
          
          <line x1="160.5" y1="202" x2="160.5" y2="246" stroke="#e2e8f0" stroke-width="0.8" />
          
          <g transform="translate(166, 202)">
            <rect x="0" y="0" width="60" height="5" fill="#475569" opacity="0.8" rx="1" />
            <rect x="0" y="8" width="60" height="3" fill="#cbd5e1" rx="0.5" />
            <rect x="0" y="14" width="60" height="3" fill="#cbd5e1" rx="0.5" />
            <rect x="0" y="20" width="60" height="3" fill="#cbd5e1" rx="0.5" />
            <rect x="0" y="26" width="60" height="3" fill="#cbd5e1" rx="0.5" />
            <rect x="0" y="32" width="60" height="3" fill="#cbd5e1" rx="0.5" />
            <rect x="0" y="38" width="45" height="3" fill="#cbd5e1" rx="0.5" />
          </g>
          
          <line x1="14" y1="254" x2="226" y2="254" stroke="#e2e8f0" stroke-width="1" />
          
          <!-- Running Footer -->
          <line x1="14" y1="308" x2="226" y2="308" stroke="#cbd5e1" stroke-width="0.8" />
          <text x="14" y="318" font-family="system-ui, -apple-system, sans-serif" font-size="7.5" font-weight="700" fill="#475569">${escapeHtml(folderName)} · ${escapeHtml(name)}</text>
          <text x="226" y="318" text-anchor="end" font-family="system-ui, -apple-system, sans-serif" font-size="7.5" font-weight="800" fill="${isIdml ? '#e11d48' : '#2563eb'}">${isIdml ? 'IDML' : 'DESIGNCRAFT'}</text>
        </svg>
      `;
    }

    return `
      <svg viewBox="0 0 200 270" xmlns="http://www.w3.org/2000/svg" style="width:100%;height:100%;max-height:190px;background:#ffffff;display:block;box-shadow:0 1px 4px rgba(0,0,0,0.06);border-radius:4px;overflow:hidden;margin:auto;">
        <rect width="200" height="270" fill="#ffffff" />
        <rect x="0.5" y="0.5" width="199" height="269" fill="none" stroke="#e2e8f0" stroke-width="1" />
        
        <rect x="12" y="12" width="176" height="246" fill="none" stroke="#f1f5f9" stroke-width="0.8" stroke-dasharray="2 2" />
        
        <!-- Top Section Bar with Folder Name -->
        <rect x="12" y="12" width="176" height="15" fill="${secColor}" opacity="0.95" rx="2" />
        <text x="18" y="22.5" font-family="system-ui, -apple-system, sans-serif" font-size="8.5" font-weight="800" fill="#ffffff" letter-spacing="0.5">${escapeHtml(secName)}</text>
        
        <!-- Headline with Template Name -->
        <rect x="12" y="32" width="176" height="15" fill="#0f172a" rx="2" />
        <text x="18" y="43" font-family="system-ui, -apple-system, sans-serif" font-size="8.5" font-weight="700" fill="#ffffff">${escapeHtml(name)}</text>
        
        <!-- Lead Photo Container -->
        <rect x="12" y="53" width="176" height="97" fill="#f8fafc" stroke="#cbd5e1" stroke-width="0.8" rx="3" />
        <g transform="translate(86, 90)" opacity="0.45">
          <rect x="-8" y="-6" width="44" height="28" fill="#e2e8f0" rx="3" />
          <circle cx="14" cy="8" r="8" fill="#94a3b8" />
          <path d="M4 18l5-6 4 4 6-7 7 9H4z" fill="#ffffff" />
        </g>
        
        <!-- Text Columns -->
        <rect x="12" y="157" width="54" height="4" fill="#64748b" opacity="0.6" rx="0.8" />
        <rect x="12" y="164" width="54" height="2.8" fill="#cbd5e1" rx="0.5" />
        <rect x="12" y="169.5" width="54" height="2.8" fill="#cbd5e1" rx="0.5" />
        <rect x="12" y="175" width="54" height="2.8" fill="#cbd5e1" rx="0.5" />
        <rect x="12" y="180.5" width="54" height="2.8" fill="#cbd5e1" rx="0.5" />
        <rect x="12" y="186" width="42" height="2.8" fill="#cbd5e1" rx="0.5" />
        
        <rect x="73" y="157" width="54" height="4" fill="#64748b" opacity="0.6" rx="0.8" />
        <rect x="73" y="164" width="54" height="2.8" fill="#cbd5e1" rx="0.5" />
        <rect x="73" y="169.5" width="54" height="2.8" fill="#cbd5e1" rx="0.5" />
        <rect x="73" y="175" width="54" height="2.8" fill="#cbd5e1" rx="0.5" />
        <rect x="73" y="180.5" width="54" height="2.8" fill="#cbd5e1" rx="0.5" />
        <rect x="73" y="186" width="48" height="2.8" fill="#cbd5e1" rx="0.5" />
        
        <rect x="134" y="157" width="54" height="4" fill="#64748b" opacity="0.6" rx="0.8" />
        <rect x="134" y="164" width="54" height="2.8" fill="#cbd5e1" rx="0.5" />
        <rect x="134" y="169.5" width="54" height="2.8" fill="#cbd5e1" rx="0.5" />
        <rect x="134" y="175" width="54" height="2.8" fill="#cbd5e1" rx="0.5" />
        <rect x="134" y="180.5" width="54" height="2.8" fill="#cbd5e1" rx="0.5" />
        <rect x="134" y="186" width="38" height="2.8" fill="#cbd5e1" rx="0.5" />
      </svg>
    `;
  }

  function getTemplateThumbnailUrl(tmpl) {
    if (!tmpl || !tmpl.id) return '';
    const cloud = typeof window !== 'undefined' ? window.DesignCraftCloud : null;
    const token = cloud?.authState?.accessToken || (cloud?.getStoredAuth?.()?.accessToken) || '';
    const tokenParam = token ? `?token=${encodeURIComponent(token)}` : '';
    if (tmpl.thumbnailUrl) {
      return tmpl.thumbnailUrl.includes('?') ? tmpl.thumbnailUrl : (tmpl.thumbnailUrl + tokenParam);
    }
    return `/api/cloud/thumbnails/${encodeURIComponent(tmpl.id)}${tokenParam}`;
  }

  function renderTemplateCardHtml(tmpl, meta) {
    const sizeKb = Math.round((tmpl.size || 0) / 1024);
    const thumbUrl = getTemplateThumbnailUrl(tmpl);
    const catName = tmpl.category || meta?.name || 'General';
    const svgWireframe = renderEditorialSvgWireframe(tmpl, meta, 'card');

    return `
      <div class="cloud-card" data-id="${tmpl.id}" data-filename="${tmpl.filename}" title="${tmpl.name}">
        <div class="cloud-thumb-box" id="thumb-box-${tmpl.id}" data-id="${tmpl.id}">
          <div class="cloud-thumb-zoom-badge" title="Pasa el cursor para ver tamaño grande">🔍</div>
          <div class="cloud-thumb-folder-badge" style="position:absolute; top:8px; left:8px; z-index:2; background:rgba(15,23,42,0.85); color:#ffffff; font-size:10.5px; font-weight:700; padding:2px 8px; border-radius:4px; backdrop-filter:blur(4px); pointer-events:none; max-width:85%; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; box-shadow:0 1px 3px rgba(0,0,0,0.3);">
            📁 ${escapeHtml(catName)}
          </div>
          ${thumbUrl ? `
            <img src="${thumbUrl}" alt="${tmpl.name}" class="cloud-thumb-img" onerror="this.style.display='none'; this.nextElementSibling.style.display='block';" />
            <div class="cloud-thumb-svg-fallback" style="display:none; width:100%; height:100%;">${svgWireframe}</div>
          ` : `
            <div class="cloud-thumb-svg-fallback" style="width:100%; height:100%;">${svgWireframe}</div>
          `}
        </div>
        <div class="cloud-card-body">
          <div class="cloud-card-header-row">
            <span class="cloud-cat-badge" title="${catName}">📁 ${catName}</span>
            <span class="cloud-type-badge ${tmpl.isIdml ? 'type-idml' : 'type-dc'}">${tmpl.isIdml ? 'IDML' : 'DesignCraft'}</span>
          </div>
          <h4 class="cloud-card-title">${tmpl.name}</h4>
          <p class="cloud-card-meta">${sizeKb > 0 ? sizeKb + ' KB · ' : ''}${tmpl.isIdml ? 'InDesign IDML' : 'DesignCraft nativo'}</p>
          <button class="cloud-btn cloud-btn-primary btn-use-template" style="margin-top: auto; padding: 8px 14px; font-size: 13px;">
            Usar Plantilla
          </button>
        </div>
      </div>
    `;
  }

  // --- FLOATING HIGH-RES HOVER PREVIEW CONTROLS ---
  let hoverTimeout = null;

  function showHoverPreview(anchorEl, tmpl) {
    const popover = document.getElementById('cloud-thumb-hover-preview');
    if (!popover || !tmpl) return;

    const meta = getCategoryMeta(tmpl.category);
    const secTag = document.getElementById('chp-sec-tag');
    const fmtTag = document.getElementById('chp-fmt-tag');
    const titleEl = document.getElementById('chp-title');
    const metaEl = document.getElementById('chp-meta');
    const pageFrame = document.getElementById('chp-page-frame');

    if (secTag) {
      secTag.textContent = `${meta?.icon || '📂'} ${(meta?.name || tmpl.category || 'GENERAL').toUpperCase()}`;
      secTag.style.background = meta?.bgTint || '#dbeafe';
      secTag.style.color = '#1e40af';
    }

    if (fmtTag) {
      if (tmpl.isIdml) {
        fmtTag.textContent = '📄 InDesign IDML';
        fmtTag.className = 'cloud-hover-fmt-tag type-idml';
      } else {
        fmtTag.textContent = '⚡ DesignCraft';
        fmtTag.className = 'cloud-hover-fmt-tag type-dc';
      }
    }

    if (titleEl) {
      titleEl.textContent = tmpl.name || 'Plantilla Editorial';
    }

    if (metaEl) {
      const sizeKb = Math.round((tmpl.size || 0) / 1024);
      metaEl.textContent = `${sizeKb > 0 ? sizeKb + ' KB · ' : ''}${tmpl.isIdml ? 'Adobe InDesign IDML' : 'DesignCraft nativo'}`;
    }

    if (pageFrame) {
      const thumbUrl = getTemplateThumbnailUrl(tmpl);
      const largeSvg = renderEditorialSvgWireframe(tmpl, meta, 'large');

      if (thumbUrl) {
        pageFrame.innerHTML = `
          <img src="${thumbUrl}" alt="${escapeHtml(tmpl.name)}" class="cloud-hover-large-img" onerror="this.style.display='none'; this.nextElementSibling.style.display='block';" />
          <div style="display:none; width:100%; height:100%;">${largeSvg}</div>
        `;
      } else {
        pageFrame.innerHTML = largeSvg;
      }
    }

    positionHoverPreview(anchorEl);
    popover.classList.add('active');
    requestAnimationFrame(() => {
      positionHoverPreview(anchorEl);
    });
  }

  function hideHoverPreview() {
    clearTimeout(hoverTimeout);
    const popover = document.getElementById('cloud-thumb-hover-preview');
    if (popover) {
      popover.classList.remove('active');
    }
  }

  function positionHoverPreview(anchorEl) {
    const popover = document.getElementById('cloud-thumb-hover-preview');
    if (!popover || !anchorEl) return;

    const rect = anchorEl.getBoundingClientRect();
    const popoverWidth = 350;
    const popoverHeight = popover.offsetHeight || 480;
    const margin = 16;

    let left = rect.right + 14;
    let transformOriginX = 'left';

    if (left + popoverWidth > window.innerWidth - margin) {
      left = rect.left - popoverWidth - 14;
      transformOriginX = 'right';
      if (left < margin) {
        left = Math.max(margin, Math.round((window.innerWidth - popoverWidth) / 2));
        transformOriginX = 'center';
      }
    }

    let top = rect.top + (rect.height / 2) - (popoverHeight / 2);
    let transformOriginY = 'center';

    if (top + popoverHeight > window.innerHeight - margin) {
      top = window.innerHeight - popoverHeight - margin;
      transformOriginY = 'bottom';
    }
    if (top < margin) {
      top = margin;
      transformOriginY = 'top';
    }

    popover.style.left = `${Math.round(left)}px`;
    popover.style.top = `${Math.round(top)}px`;
    popover.style.transformOrigin = `${transformOriginX} ${transformOriginY}`;
  }

  function attachHoverPreviewListeners(container) {
    if (!container) return;

    // 1. Tarjetas completas de maquetas (en carpeta o búsqueda)
    container.querySelectorAll('.cloud-card').forEach(card => {
      const tmplId = card.dataset.id;
      const thumbBox = card.querySelector('.cloud-thumb-box') || card;

      thumbBox.addEventListener('mouseenter', () => {
        clearTimeout(hoverTimeout);
        hoverTimeout = setTimeout(() => {
          const tmpl = cachedTemplates.find(t => t.id === tmplId);
          if (tmpl) {
            showHoverPreview(thumbBox, tmpl);
          }
        }, 120);
      });

      thumbBox.addEventListener('mouseleave', () => {
        clearTimeout(hoverTimeout);
        hideHoverPreview();
      });
    });

    // 2. Mini-miniaturas en la tira de vista previa de las tarjetas de carpeta
    container.querySelectorAll('.cloud-folder-mini-thumb').forEach(miniThumb => {
      const tmplId = miniThumb.dataset.id;
      if (!tmplId) return;

      miniThumb.addEventListener('mouseenter', (e) => {
        e.stopPropagation();
        clearTimeout(hoverTimeout);
        hoverTimeout = setTimeout(() => {
          const tmpl = cachedTemplates.find(t => t.id === tmplId);
          if (tmpl) {
            showHoverPreview(miniThumb, tmpl);
          }
        }, 120);
      });

      miniThumb.addEventListener('mouseleave', (e) => {
        e.stopPropagation();
        clearTimeout(hoverTimeout);
        hideHoverPreview();
      });

      miniThumb.addEventListener('click', (e) => {
        e.stopPropagation();
        hideHoverPreview();
        const tmpl = cachedTemplates.find(t => t.id === tmplId);
        if (tmpl) {
          const meta = getCategoryMeta(tmpl.category);
          promptAssignEditionModal(tmpl, meta?.standardSection, meta?.defaultPage);
        }
      });
    });
  }

  function attachTemplateCardClickListeners(parentMeta) {
    const contentArea = document.getElementById('templates-content-area');
    if (!contentArea) return;

    contentArea.querySelectorAll('.cloud-card').forEach(card => {
      card.addEventListener('click', () => {
        hideHoverPreview();
        const fileId = card.dataset.id;
        const tmpl = cachedTemplates.find(t => t.id === fileId);
        if (tmpl) {
          const meta = parentMeta || getCategoryMeta(tmpl.category);
          promptAssignEditionModal(tmpl, meta?.standardSection, meta?.defaultPage);
        }
      });
    });
  }

  function uint8ToBase64(u8) {
    let binary = '';
    const len = u8.byteLength;
    for (let i = 0; i < len; i++) {
      binary += String.fromCharCode(u8[i]);
    }
    return window.btoa(binary);
  }

  const openModal = () => {
    overlay.classList.add('open');
    renderModal();
  };
  const closeModal = () => {
    overlay.classList.remove('open');
    hideHoverPreview();
  };

  pillBtn.addEventListener('click', () => {
    if (!window.DesignCraftCloud?.authState?.isAuthenticated) {
      window.DesignCraftCloud?.signIn('consent');
      return;
    }
    openModal();
  });
  closeBtn.addEventListener('click', closeModal);

  overlay.addEventListener('click', (e) => {
    if (e.target === overlay) closeModal();
  });

  // Ocultar vista previa ampliada si el usuario hace scroll, redimensiona o presiona Escape
  document.getElementById('cloud-modal-content')?.addEventListener('scroll', hideHoverPreview, { passive: true });
  window.addEventListener('scroll', hideHoverPreview, { passive: true });
  window.addEventListener('resize', hideHoverPreview, { passive: true });
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') hideHoverPreview();
  });

  // Manejo de tabs
  tabsBar.querySelectorAll('.cloud-tab-btn').forEach(btn => {
    btn.addEventListener('click', (e) => {
      hideHoverPreview();
      tabsBar.querySelectorAll('.cloud-tab-btn').forEach(b => b.classList.remove('active'));
      btn.classList.add('active');
      currentTab = btn.dataset.tab;
      renderModal();
    });
  });

  if (window.DesignCraftCloud) {
    window.DesignCraftCloud.subscribe((state) => {
      updatePillUI(state);
      if (overlay.classList.contains('open')) {
        renderModal();
      }
    });
  }

  function updatePillUI(state) {
    const pill = document.getElementById('cloud-pill-btn');
    const iconSpan = document.getElementById('cloud-pill-icon');
    const textSpan = document.getElementById('cloud-pill-text');
    if (!pill || !iconSpan || !textSpan) return;

    if (state.isAuthenticated && state.user) {
      pill.style.borderColor = '#86efac';
      pill.style.background = '#f0fdf4';
      pill.style.color = '#166534';
      pill.title = `Google Drive Conectado como ${state.user.name || state.user.email || ''}. Clic para abrir el catálogo.`;
      iconSpan.innerHTML = state.user.picture
        ? `<img src="${state.user.picture}" alt="avatar" style="width:20px;height:20px;border-radius:50%;object-fit:cover;" />`
        : '<span style="color:#16a34a;font-size:13px;">🟢</span>';
      textSpan.innerHTML = `<span style="font-weight:600;">${state.user.name || 'Conectado'}</span> <span style="font-size:10px;font-weight:700;color:#15803d;background:#dcfce7;padding:2px 6px;border-radius:8px;">DRIVE</span>`;
    } else {
      pill.style.borderColor = '#ef4444';
      pill.style.background = '#fef2f2';
      pill.style.color = '#991b1b';
      pill.title = 'Google Drive Desconectado / Sesión Expirada. Clic para conectar tu cuenta.';
      iconSpan.innerHTML = '<span style="color:#dc2626;font-size:13px;">🔴</span>';
      textSpan.innerHTML = '<span style="font-weight:700;">Google Desconectado</span> <span style="font-size:11px;font-weight:700;color:#ffffff;background:#dc2626;padding:2px 8px;border-radius:10px;margin-left:4px;box-shadow:0 1px 3px rgba(220,38,38,0.3);">Conectar</span>';
    }
  }

  async function renderModal() {
    const cloud = window.DesignCraftCloud;
    if (!cloud) return;

    const modalTitleEl = document.getElementById('cloud-modal-header-title');
    if (modalTitleEl) {
      const provider = cloud.config?.provider === 'nextcloud' ? 'Nextcloud' : 'Google Drive';
      modalTitleEl.textContent = currentTab === 'templates'
        ? `Plantillas de ${provider}`
        : 'Guardar en Edición del Día';
    }

    tabsBar.style.display = currentTab === 'save' ? 'flex' : 'none';

    if (currentTab === 'templates') {
      renderTemplatesTab();
    } else {
      if (!cloud.authState.isAuthenticated) {
        modalContent.innerHTML = `
          <div style="text-align: center; padding: 40px 16px;">
            <div style="font-size: 48px; margin-bottom: 12px;">☁</div>
            <h4 style="margin: 0 0 8px 0; font-size: 18px; color: #0f172a;">Iniciar Sesión para Guardar en Drive</h4>
            <p style="margin: 0 auto 24px auto; max-width: 440px; font-size: 13.5px; color: #64748b; line-height: 1.6;">
              Inicia sesión con tu cuenta de Google para enviar la maqueta y el PDF a la carpeta de la edición del día.
            </p>
            <button class="cloud-btn cloud-btn-primary" id="btn-cloud-login-save" style="margin: 0 auto; width: 280px; padding: 12px 20px; font-size: 14px;">
              Iniciar Sesión con Google
            </button>
          </div>
        `;
        document.getElementById('btn-cloud-login-save')?.addEventListener('click', () => {
          cloud.signIn();
        });
        return;
      }
      renderSaveTab();
    }
  }

  // --- TAB 1: CATÁLOGO DE PLANTILLAS (BACKEND CREADO / CACHEADO) ---
  async function renderTemplatesTab(forceSync = false) {
    const cloud = window.DesignCraftCloud;
    if (cloud) {
      await cloud.getValidAccessToken();
    }
    const auth = cloud.authState;

    modalContent.innerHTML = `
      ${!auth.isAuthenticated ? `
        <div style="background: #fef2f2; border: 1.5px solid #f87171; color: #991b1b; padding: 12px 16px; border-radius: 10px; margin-bottom: 16px; display: flex; align-items: center; justify-content: space-between; gap: 14px; box-shadow: 0 2px 8px rgba(220, 38, 38, 0.08);">
          <div style="display: flex; align-items: center; gap: 10px;">
            <span style="font-size: 22px;">🔑</span>
            <div>
              <div style="font-weight: 700; font-size: 13.5px;">Google Drive Desconectado / Sesión Expirada</div>
              <div style="font-size: 12.5px; color: #7f1d1d; line-height: 1.4;">Debes iniciar sesión con Google para poder descargar y abrir las plantillas de la nube en tu maqueta.</div>
            </div>
          </div>
          <button class="cloud-btn cloud-btn-primary" id="btn-banner-auth" style="padding: 8px 16px; font-size: 13px; font-weight: 700; background: #2563eb; color: #ffffff; border: none; border-radius: 8px; cursor: pointer; white-space: nowrap;">
            🔑 Iniciar Sesión con Google
          </button>
        </div>
      ` : ''}

      <div id="templates-loading-box" style="text-align: center; padding: 60px 20px; color: #64748b;">
        <div style="font-size: 36px; margin-bottom: 12px;">⏳</div>
        <div style="font-weight: 600; color: #0f172a; margin-bottom: 6px; font-size: 15px;" id="templates-loading-title">Cargando catálogo...</div>
        <div style="font-size: 13px; color: #64748b;" id="templates-loading-subtext">Obteniendo carpetas y miniaturas organizadas por secciones...</div>
      </div>

      <div id="templates-content-area" style="display: none; flex-direction: column; gap: 14px; flex: 1;"></div>
    `;

    document.getElementById('btn-auth-drive')?.addEventListener('click', () => {
      cloud.signIn();
    });

    document.getElementById('btn-banner-auth')?.addEventListener('click', () => {
      cloud.signIn();
    });

    document.getElementById('btn-reload-templates')?.addEventListener('click', async () => {
      const validToken = await cloud.getValidAccessToken();
      if (!validToken) {
        cloud.signIn();
        return;
      }
      cachedTemplates = [];
      cachedCategories = [];
      activeFolderId = null;
      renderTemplatesTab(true);
    });

    const loadingBox = document.getElementById('templates-loading-box');
    const loadingTitle = document.getElementById('templates-loading-title');
    const loadingSubtext = document.getElementById('templates-loading-subtext');
    const contentArea = document.getElementById('templates-content-area');

    try {
      const folderIdParam = cloud.config?.templatesFolderId || undefined;
      const validToken = (await cloud.getValidAccessToken()) || auth.accessToken;
      if (forceSync) {
        if (!validToken) {
          cloud.signIn();
          return;
        }
        if (loadingTitle) loadingTitle.textContent = 'Sincronizando con Google Drive en el Servidor...';
        if (loadingSubtext) loadingSubtext.textContent = 'El backend está explorando todas las carpetas y subcarpetas con plantillas...';
        try {
          const syncRes = await fetch('/api/cloud/sync', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ accessToken: validToken, folderId: folderIdParam, forceRefresh: true }),
          });
          if (syncRes.ok) {
            const syncData = await syncRes.json();
            if (syncData.syncSuccess === false && syncData.syncError) {
              const err = (syncData.syncError || '').toLowerCase();
              if (err.includes('invalid') || err.includes('insufficient') || err.includes('401') || err.includes('403')) {
                console.warn('[CloudUI] Token de Google Drive expirado o con permisos insuficientes:', syncData.syncError);
                cloud.signIn();
                return;
              }
            }
            if (syncData.templates && syncData.templates.length > 0) {
              cachedTemplates = syncData.templates;
              cachedCategories = syncData.categories || [];
            }
          }
        } catch (syncErr) {
          console.warn('[CloudUI] Falló sync forzado:', syncErr);
        }
      } else if (cachedTemplates.length === 0) {
        // Fast backend catalog fetch (1ms)
        try {
          const catRes = await fetch('/api/cloud/catalog');
          if (catRes.ok) {
            const catData = await catRes.json();
            cachedTemplates = catData.templates || [];
            cachedCategories = catData.categories || [];
          }
        } catch (catErr) {
          console.warn('[CloudUI] Error cargando /api/cloud/catalog:', catErr);
        }
      }

      // Si el usuario tiene sesión de Google Drive pero aún no tenemos plantillas de Drive en caché,
      // sincronizamos automáticamente para cargar todas las carpetas temáticas:
      const hasDriveTemplates = cachedTemplates.some(t => t.source === 'drive');
      if (validToken && (!hasDriveTemplates || cachedTemplates.length === 0)) {
        if (loadingTitle) loadingTitle.textContent = 'Sincronizando carpetas de Google Drive...';
        if (loadingSubtext) loadingSubtext.textContent = 'Explorando todas las carpetas y subcarpetas con plantillas...';
        try {
          const syncRes = await fetch('/api/cloud/sync', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ accessToken: validToken, folderId: folderIdParam }),
          });
          if (syncRes.ok) {
            const syncData = await syncRes.json();
            if (syncData.templates && syncData.templates.length > 0) {
              cachedTemplates = syncData.templates;
              cachedCategories = syncData.categories || [];
            }
          }
        } catch (syncErr) {
          console.warn('[CloudUI] Falló sync automático con Google Drive:', syncErr);
        }
      }

      loadingBox.style.display = 'none';
      contentArea.style.display = 'flex';

      function renderCatalogView() {
        // 1. Recopilar categorías únicas
        const catNamesSet = new Set();
        const categoriesList = [];

        (cachedCategories || []).forEach(c => {
          const name = (c.name || c.id || '').trim();
          if (name && name !== 'all' && !catNamesSet.has(name)) {
            catNamesSet.add(name);
            categoriesList.push({ id: c.id || name, name: name });
          }
        });

        (cachedTemplates || []).forEach(t => {
          const name = (t.category || '').trim();
          if (name && !catNamesSet.has(name)) {
            catNamesSet.add(name);
            categoriesList.push({ id: name, name: name });
          }
        });

        // Categoría activa por defecto (como en IDML Injector: primera categoría)
        if (activeFolderId === null && categoriesList.length > 0) {
          activeFolderId = categoriesList[0].name;
        }

        // Paleta exacta de 14 colores de IDML Injector
        const palette = [
          { bg: '#4f46e5', hover: '#4338ca', text: '#ffffff', unselected: '#e0e7ff', unselectedText: '#4338ca', unselectedHover: '#c7d2fe' }, // indigo
          { bg: '#9333ea', hover: '#7e22ce', text: '#ffffff', unselected: '#f3e8ff', unselectedText: '#7e22ce', unselectedHover: '#e9d5ff' }, // purple
          { bg: '#db2777', hover: '#be185d', text: '#ffffff', unselected: '#fce7f3', unselectedText: '#be185d', unselectedHover: '#fbcfe8' }, // pink
          { bg: '#dc2626', hover: '#b91c1c', text: '#ffffff', unselected: '#fee2e2', unselectedText: '#b91c1c', unselectedHover: '#fecaca' }, // red
          { bg: '#ea580c', hover: '#c2410c', text: '#ffffff', unselected: '#ffedd5', unselectedText: '#c2410c', unselectedHover: '#fed7aa' }, // orange
          { bg: '#d97706', hover: '#b45309', text: '#ffffff', unselected: '#fef3c7', unselectedText: '#b45309', unselectedHover: '#fde68a' }, // amber
          { bg: '#ca8a04', hover: '#a16207', text: '#ffffff', unselected: '#fef9c3', unselectedText: '#a16207', unselectedHover: '#fef08a' }, // yellow
          { bg: '#65a30d', hover: '#4d7c0f', text: '#ffffff', unselected: '#ecfccb', unselectedText: '#4d7c0f', unselectedHover: '#d9f99d' }, // lime
          { bg: '#16a34a', hover: '#15803d', text: '#ffffff', unselected: '#dcfce7', unselectedText: '#15803d', unselectedHover: '#bbf7d0' }, // green
          { bg: '#059669', hover: '#047857', text: '#ffffff', unselected: '#d1fae5', unselectedText: '#047857', unselectedHover: '#a7f3d0' }, // emerald
          { bg: '#0d9488', hover: '#0f766e', text: '#ffffff', unselected: '#ccfbf1', unselectedText: '#0f766e', unselectedHover: '#99f6e4' }, // teal
          { bg: '#0891b2', hover: '#0e7490', text: '#ffffff', unselected: '#cffafe', unselectedText: '#0e7490', unselectedHover: '#a5f3fc' }, // cyan
          { bg: '#0284c7', hover: '#0369a1', text: '#ffffff', unselected: '#e0f2fe', unselectedText: '#0369a1', unselectedHover: '#bae6fd' }, // sky
          { bg: '#2563eb', hover: '#1d4ed8', text: '#ffffff', unselected: '#dbeafe', unselectedText: '#1d4ed8', unselectedHover: '#bfdbfe' }, // blue
        ];

        function getCatColor(catName, isSelected) {
          let hash = 0;
          for (let i = 0; i < catName.length; i++) {
            hash = catName.charCodeAt(i) + ((hash << 5) - hash);
          }
          const idx = Math.abs(hash) % palette.length;
          const c = palette[idx];
          return isSelected
            ? { bg: c.bg, text: c.text, hover: c.hover }
            : { bg: c.unselected, text: c.unselectedText, hover: c.unselectedHover };
        }

        const q = (activeSearchQuery || '').toLowerCase().trim();
        const filtered = cachedTemplates.filter(t => {
          const cat = (t.category || 'General').trim();
          const matchCat = !activeFolderId || cat.toLowerCase() === activeFolderId.toLowerCase();
          const matchQuery = !q || t.name.toLowerCase().includes(q) || cat.toLowerCase().includes(q) || (t.filename && t.filename.toLowerCase().includes(q));
          return matchCat && matchQuery;
        });

        const pillsHtml = categoriesList.map(cat => {
          const isSelected = activeFolderId && activeFolderId.toLowerCase() === cat.name.toLowerCase();
          const c = getCatColor(cat.name, isSelected);
          return `
            <button
              type="button"
              class="clean-cat-pill ${isSelected ? 'active' : ''}"
              data-cat="${escapeHtml(cat.name)}"
              style="
                background: ${c.bg};
                color: ${c.text};
                padding: 8px 18px;
                border-radius: 8px;
                font-weight: ${isSelected ? '700' : '600'};
                font-size: 13px;
                border: none;
                cursor: pointer;
                transition: all 0.15s ease;
                letter-spacing: 0.3px;
                text-transform: uppercase;
              "
              onmouseover="this.style.background='${c.hover}'"
              onmouseout="this.style.background='${c.bg}'"
            >
              ${escapeHtml(cat.name)}
            </button>
          `;
        }).join('');

        contentArea.innerHTML = `
          <!-- Sección de Categorías -->
          <div style="margin-bottom: 12px;">
            <div style="display: flex; align-items: center; justify-content: space-between; margin-bottom: 12px;">
              <div style="display: flex; align-items: center; gap: 8px;">
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="#4f46e5" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path>
                </svg>
                <h3 style="margin: 0; font-size: 16px; font-weight: 600; color: #374151;">Categorías</h3>
              </div>
              <button id="btn-reload-templates-clean" title="Sincronizar plantillas con Google Drive" style="background: none; border: 1px solid #e2e8f0; border-radius: 6px; padding: 5px 12px; font-size: 12.5px; font-weight: 500; color: #64748b; cursor: pointer; display: flex; align-items: center; gap: 5px; transition: all 0.15s ease;">
                🔄 Sincronizar
              </button>
            </div>
            <div style="display: flex; flex-wrap: wrap; gap: 8px;">
              ${pillsHtml}
            </div>
          </div>

          <!-- Buscador de Plantillas -->
          <div style="position: relative; width: 100%; margin: 6px 0 16px 0;">
            <svg style="position: absolute; left: 14px; top: 50%; transform: translateY(-50%); color: #9ca3af; pointer-events: none;" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <circle cx="11" cy="11" r="8"></circle>
              <line x1="21" y1="21" x2="16.65" y2="16.65"></line>
            </svg>
            <input
              type="text"
              id="clean-search-input"
              class="clean-search-input"
              placeholder="Buscar plantillas..."
              value="${escapeHtml(activeSearchQuery)}"
              style="width: 100%; box-sizing: border-box; padding: 10px 16px 10px 42px; border: 1px solid #d1d5db; border-radius: 8px; font-size: 14px; color: #111827; background: #ffffff; outline: none; transition: border-color 0.15s ease, box-shadow 0.15s ease;"
            />
          </div>

          <!-- Cuadrícula de Tarjetas de Plantillas -->
          ${filtered.length === 0 ? `
            <div style="text-align: center; padding: 60px 16px; color: #6b7280; background: #f9fafb; border-radius: 12px; border: 1px dashed #e5e7eb; margin: 12px 0;">
              <div style="font-size: 32px; margin-bottom: 8px;">📄</div>
              <div style="font-weight: 600; color: #111827; font-size: 15px; margin-bottom: 4px;">
                ${activeSearchQuery ? 'No se encontraron plantillas que coincidan con la búsqueda' : 'No hay plantillas en esta categoría'}
              </div>
              <div style="font-size: 13px; color: #6b7280;">Selecciona otra categoría o cambia el término de búsqueda.</div>
            </div>
          ` : `
            <div class="clean-templates-grid" style="display: grid; grid-template-columns: repeat(auto-fill, minmax(290px, 1fr)); gap: 16px; padding-bottom: 24px;">
              ${filtered.map(tmpl => {
                const thumbUrl = getTemplateThumbnailUrl(tmpl);
                const meta = getCategoryMeta(tmpl.category);
                const catName = tmpl.category || meta?.name || 'General';
                const svgWireframe = renderEditorialSvgWireframe(tmpl, meta, 'card');

                return `
                  <div
                    class="clean-template-card"
                    data-id="${tmpl.id}"
                    title="${escapeHtml(tmpl.name)}"
                  >
                    <div class="clean-card-thumb-container">
                      ${thumbUrl ? `
                        <img
                          src="${thumbUrl}"
                          alt="${escapeHtml(tmpl.name)}"
                          class="clean-card-img"
                          onerror="this.style.display='none'; this.nextElementSibling.style.display='flex';"
                        />
                        <div style="display:none; width:100%; height:100%; align-items:center; justify-content:center;">
                          ${svgWireframe}
                        </div>
                      ` : `
                        <div style="width:100%; height:100%; display:flex; align-items:center; justify-content:center;">
                          ${svgWireframe}
                        </div>
                      `}
                      <button
                        type="button"
                        class="clean-zoom-btn"
                        data-id="${tmpl.id}"
                        title="Ver a tamaño real"
                      >
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="#4f46e5" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                          <circle cx="11" cy="11" r="8"></circle>
                          <line x1="21" y1="21" x2="16.65" y2="16.65"></line>
                          <line x1="11" y1="8" x2="11" y2="14"></line>
                          <line x1="8" y1="11" x2="14" y2="11"></line>
                        </svg>
                      </button>
                    </div>
                    <div class="clean-card-info" style="padding: 16px; background: #ffffff; border-top: 1px solid #f3f4f6; display: flex; flex-direction: column; gap: 4px;">
                      <h4 style="font-weight: 700; font-size: 16px; color: #111827; margin: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">
                        ${escapeHtml(tmpl.name)}
                      </h4>
                      <p style="font-size: 13px; font-weight: 600; color: #6b7280; text-transform: uppercase; letter-spacing: 0.5px; margin: 0;">
                        ${escapeHtml(catName)}
                      </p>
                    </div>
                  </div>
                `;
              }).join('')}
            </div>
          `}
        `;

        // Listeners
        contentArea.querySelectorAll('.clean-cat-pill').forEach(pill => {
          pill.addEventListener('click', () => {
            activeFolderId = pill.dataset.cat;
            renderCatalogView();
          });
        });

        const searchInput = document.getElementById('clean-search-input');
        if (searchInput) {
          searchInput.addEventListener('input', (e) => {
            activeSearchQuery = e.target.value;
            renderCatalogView();
            const reInput = document.getElementById('clean-search-input');
            if (reInput) {
              reInput.focus();
              reInput.setSelectionRange(reInput.value.length, reInput.value.length);
            }
          });
        }

        document.getElementById('btn-reload-templates-clean')?.addEventListener('click', async () => {
          const validToken = await cloud.getValidAccessToken();
          if (!validToken) {
            cloud.signIn();
            return;
          }
          cachedTemplates = [];
          cachedCategories = [];
          activeFolderId = null;
          renderTemplatesTab(true);
        });

        contentArea.querySelectorAll('.clean-zoom-btn').forEach(btn => {
          btn.addEventListener('click', (e) => {
            e.stopPropagation();
            const tmplId = btn.dataset.id;
            const tmpl = cachedTemplates.find(t => t.id === tmplId);
            if (tmpl) {
              openCleanZoom(tmpl);
            }
          });
        });

        contentArea.querySelectorAll('.clean-template-card').forEach(card => {
          card.addEventListener('click', () => {
            const tmplId = card.dataset.id;
            const tmpl = cachedTemplates.find(t => t.id === tmplId);
            if (tmpl) {
              const meta = getCategoryMeta(tmpl.category);
              promptAssignEditionModal(tmpl, meta?.standardSection, meta?.defaultPage);
            }
          });
        });
      }

      function openCleanZoom(tmpl) {
        if (!tmpl) return;
        const thumbUrl = getTemplateThumbnailUrl(tmpl);
        const meta = getCategoryMeta(tmpl.category);
        const catName = tmpl.category || meta?.name || 'General';

        let zoomModal = document.getElementById('clean-zoom-modal');
        if (!zoomModal) {
          zoomModal = document.createElement('div');
          zoomModal.id = 'clean-zoom-modal';
          zoomModal.style.cssText = 'position:fixed;inset:0;background:rgba(0,0,0,0.8);backdrop-filter:blur(4px);z-index:1000000;display:flex;align-items:center;justify-content:center;padding:16px;box-sizing:border-box;';
          document.body.appendChild(zoomModal);
        }

        zoomModal.innerHTML = `
          <div style="position:relative;max-width:95vw;max-height:95vh;background:#ffffff;border-radius:12px;overflow:hidden;display:flex;flex-direction:column;box-shadow:0 25px 60px rgba(0,0,0,0.5);animation:cloudModalIn 0.2s ease;">
            <button id="btn-close-clean-zoom" type="button" style="position:absolute;top:10px;right:12px;padding:6px 12px;background:rgba(255,255,255,0.9);border:none;border-radius:8px;box-shadow:0 2px 6px rgba(0,0,0,0.15);font-size:20px;color:#374151;cursor:pointer;z-index:30;line-height:1;" title="Cerrar">✕</button>
            <div style="padding:16px 22px;border-bottom:1px solid #f1f5f9;background:#ffffff;">
              <h3 style="margin:0 0 2px 0;font-size:18px;font-weight:700;color:#111827;">${escapeHtml(tmpl.name)}</h3>
              <p style="margin:0;font-size:13px;font-weight:600;color:#6b7280;text-transform:uppercase;">${escapeHtml(catName)}</p>
            </div>
            <div style="overflow:auto;max-height:calc(95vh - 95px);background:#f3f4f6;display:flex;align-items:center;justify-content:center;padding:16px;">
              ${thumbUrl ? `
                <img src="${thumbUrl}" alt="${escapeHtml(tmpl.name)}" style="max-width:100%;max-height:100%;object-fit:contain;box-shadow:0 10px 30px rgba(0,0,0,0.12);" />
              ` : `
                <div style="width:500px;max-width:100%;">${renderEditorialSvgWireframe(tmpl, meta, 'large')}</div>
              `}
            </div>
          </div>
        `;

        zoomModal.style.display = 'flex';

        zoomModal.onclick = (e) => {
          if (e.target === zoomModal) {
            zoomModal.style.display = 'none';
          }
        };
        zoomModal.querySelector('#btn-close-clean-zoom').onclick = () => {
          zoomModal.style.display = 'none';
        };
      }


      renderCatalogView();

    } catch (err) {
      loadingBox.innerHTML = `
        <div style="font-size: 32px; margin-bottom: 12px; color: #dc2626;">⚠️</div>
        <div style="font-weight: 600; color: #dc2626; margin-bottom: 6px;">Error al cargar catálogo</div>
        <div style="font-size: 13px; color: #64748b; margin-bottom: 16px;">${err.message}</div>
        <button class="cloud-btn cloud-btn-primary" id="btn-retry-templates" style="margin: 0 auto;">
          🔄 Reintentar
        </button>
      `;
      document.getElementById('btn-retry-templates')?.addEventListener('click', () => {
        cachedTemplates = [];
        renderTemplatesTab();
      });
    }
  }

  // Modal obligatorio de asignación a la Edición antes de cargar la plantilla
  function promptAssignEditionModal(tmpl, defaultSection, defaultPage) {
    hideHoverPreview();
    const existingModal = document.getElementById('assign-edition-overlay');
    if (existingModal) existingModal.remove();

    const today = new Date();
    const yyyy = today.getFullYear();
    const mm = String(today.getMonth() + 1).padStart(2, '0');
    const dd = String(today.getDate()).padStart(2, '0');
    const todayStr = `${yyyy}-${mm}-${dd}`;

    let versionSuffix = '';

    const overlay = document.createElement('div');
    overlay.id = 'assign-edition-overlay';
    overlay.style.cssText = 'position:fixed;inset:0;background:rgba(15,23,42,0.65);backdrop-filter:blur(5px);z-index:100000;display:flex;align-items:center;justify-content:center;font-family:system-ui,-apple-system,sans-serif;padding:16px;box-sizing:border-box;';

    overlay.innerHTML = `
      <div style="background:#ffffff;border-radius:16px;width:540px;max-width:100%;box-shadow:0 25px 50px -12px rgba(0,0,0,0.35);overflow:hidden;display:flex;flex-direction:column;animation:cloudModalIn 0.2s ease-out;">
        <!-- Header -->
        <div style="padding:18px 24px;background:#f8fafc;border-bottom:1px solid #e2e8f0;display:flex;justify-content:space-between;align-items:center;">
          <div>
            <h3 style="margin:0;font-size:16px;font-weight:700;color:#0f172a;">📰 Asignar Página a la Edición</h3>
            <p style="margin:3px 0 0 0;font-size:12.5px;color:#64748b;">Plantilla: <strong style="color:#2563eb;">${tmpl.name}</strong></p>
          </div>
          <button id="btn-close-assign" type="button" style="background:none;border:none;font-size:20px;color:#94a3b8;cursor:pointer;padding:4px;border-radius:6px;">✕</button>
        </div>

        <!-- Body -->
        <div style="padding:22px 24px;display:flex;flex-direction:column;gap:16px;">

          <!-- 1. Fecha de Edición -->
          <div>
            <label style="display:block;font-size:12.5px;font-weight:600;color:#334155;margin-bottom:6px;">
              1. Fecha de la Edición del Diario
            </label>
            <input type="date" id="assign-edition-date" value="${todayStr}" style="width:100%;box-sizing:border-box;padding:9px 12px;border:1.5px solid #cbd5e1;border-radius:8px;font-size:14px;color:#0f172a;background:#fff;" />
          </div>

          <!-- 2. Número de Página y Sección -->
          <div style="display:flex;gap:12px;">
            <div style="flex:1;">
              <label style="display:block;font-size:12.5px;font-weight:600;color:#334155;margin-bottom:6px;">
                2. Número de Página
              </label>
              <input type="number" id="assign-page-num" min="1" max="99" value="${defaultPage || 3}" style="width:100%;box-sizing:border-box;padding:9px 12px;border:1.5px solid #cbd5e1;border-radius:8px;font-size:14px;color:#0f172a;background:#fff;" />
            </div>

            <div style="flex:2;">
              <label style="display:block;font-size:12.5px;font-weight:600;color:#334155;margin-bottom:6px;">
                3. Sección Editorial
              </label>
              <select id="assign-section-select" style="width:100%;box-sizing:border-box;padding:9px 12px;border:1.5px solid #cbd5e1;border-radius:8px;font-size:14px;background:#fff;color:#0f172a;">
                <option value="OPINION">OPINION</option>
                <option value="PORTADA">PORTADA</option>
                <option value="POLITICA">POLITICA</option>
                <option value="ECONOMIA">ECONOMIA</option>
                <option value="LOCAL">LOCAL</option>
                <option value="INTERNACIONAL">INTERNACIONAL</option>
                <option value="DEPORTES">DEPORTES</option>
                <option value="JUDICIAL">JUDICIAL</option>
                <option value="ESPECIALES">ESPECIALES</option>
                <option value="PUNTO_FINAL">PUNTO_FINAL</option>
                <option value="EDITORIAL">EDITORIAL</option>
                <option value="REVISTA">REVISTA</option>
                <option value="CULTURA">CULTURA</option>
                <option value="SOCIEDAD">SOCIEDAD</option>
                <option value="SOCIALES">SOCIALES</option>
                <option value="ESPECTACULOS">ESPECTACULOS</option>
                <option value="CLASIFICADOS">CLASIFICADOS</option>
                <option value="GENERAL">GENERAL</option>
                <option value="__custom__">-- Escribir otra sección... --</option>
              </select>
              <input type="text" id="assign-section-custom" placeholder="Nombre de sección..." style="display:none;margin-top:6px;width:100%;box-sizing:border-box;padding:7px 10px;border:1.5px solid #cbd5e1;border-radius:8px;font-size:13px;" />
            </div>
          </div>

          <!-- 4. Vista previa de Nomenclatura Estándar -->
          <div style="background:#f8fafc;border:1.5px dashed #94a3b8;border-radius:10px;padding:12px 14px;">
            <div style="font-size:11px;font-weight:700;color:#64748b;text-transform:uppercase;letter-spacing:0.5px;margin-bottom:4px;">
              Nomenclatura Estándar del Archivo
            </div>
            <div id="assign-preview-name" style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-size:14px;font-weight:700;color:#1e293b;word-break:break-all;">
              ...
            </div>
          </div>

          <!-- 5. Alerta de Colisión (oculta por defecto) -->
          <div id="assign-collision-box" style="display:none;background:#fef2f2;border:1.5px solid #ef4444;border-radius:10px;padding:12px 14px;color:#991b1b;">
            <div style="font-weight:700;margin-bottom:4px;display:flex;align-items:center;gap:6px;font-size:13.5px;">
              <span>⚠️</span> <span>Esta página ya existe en la edición</span>
            </div>
            <p style="margin:0 0 10px 0;font-size:12.5px;color:#7f1d1d;line-height:1.4;">
              Ya existe un documento guardado para esta fecha y página. Elige una acción:
            </p>
            <div style="display:flex;gap:8px;">
              <button id="btn-assign-open-existing" type="button" style="background:#ffffff;color:#991b1b;border:1.5px solid #ef4444;border-radius:6px;padding:7px 12px;font-size:12px;font-weight:600;cursor:pointer;">
                📂 Abrir página existente
              </button>
              <button id="btn-assign-new-version" type="button" style="background:#dc2626;color:#ffffff;border:none;border-radius:6px;padding:7px 12px;font-size:12px;font-weight:600;cursor:pointer;">
                ➕ Crear versión 2 (_v2)
              </button>
            </div>
          </div>

          <!-- 6. Alerta de Desconexión de Google Drive -->
          <div id="assign-auth-warning" style="display:none;background:#fef2f2;border:1.5px solid #ef4444;border-radius:10px;padding:12px 14px;color:#991b1b;align-items:center;justify-content:space-between;gap:12px;">
            <div style="display:flex;align-items:center;gap:10px;">
              <span style="font-size:22px;">🔑</span>
              <div>
                <div style="font-weight:700;font-size:13.5px;">Google Drive Desconectado / Sesión Expirada</div>
                <div style="font-size:12px;color:#7f1d1d;line-height:1.4;">Debes iniciar sesión con Google para autorizar la descarga de esta maqueta.</div>
              </div>
            </div>
            <button id="btn-assign-auth" type="button" style="background:#2563eb;color:#ffffff;border:none;border-radius:8px;padding:8px 14px;font-size:12.5px;font-weight:700;cursor:pointer;white-space:nowrap;box-shadow:0 2px 8px rgba(37,99,235,0.3);">
              🔑 Conectar Google
            </button>
          </div>

        </div>

        <!-- Footer -->
        <div style="padding:16px 24px;background:#f8fafc;border-top:1px solid #e2e8f0;display:flex;justify-content:flex-end;gap:10px;">
          <button id="btn-cancel-assign" type="button" style="background:#ffffff;color:#475569;border:1px solid #cbd5e1;border-radius:8px;padding:9px 16px;font-size:13px;font-weight:600;cursor:pointer;">
            Cancelar
          </button>
          <button id="btn-confirm-assign" type="button" style="background:#2563eb;color:#ffffff;border:none;border-radius:8px;padding:9px 20px;font-size:13px;font-weight:600;cursor:pointer;box-shadow:0 4px 12px rgba(37,99,235,0.25);">
            ✍ Crear y Abrir en Modo Redactor
          </button>
        </div>
      </div>
    `;

    document.body.appendChild(overlay);

    const inputDate = overlay.querySelector('#assign-edition-date');
    const inputPage = overlay.querySelector('#assign-page-num');
    const selectSec = overlay.querySelector('#assign-section-select');
    const customSec = overlay.querySelector('#assign-section-custom');
    const previewName = overlay.querySelector('#assign-preview-name');
    const collisionBox = overlay.querySelector('#assign-collision-box');
    const authWarning = overlay.querySelector('#assign-auth-warning');
    const btnAssignAuth = overlay.querySelector('#btn-assign-auth');
    const btnOpenExisting = overlay.querySelector('#btn-assign-open-existing');
    const btnNewVersion = overlay.querySelector('#btn-assign-new-version');
    const btnConfirm = overlay.querySelector('#btn-confirm-assign');
    const btnCancel = overlay.querySelector('#btn-cancel-assign');
    const btnClose = overlay.querySelector('#btn-close-assign');

    const cloud = window.DesignCraftCloud;
    let unsubCloudAuth = null;
    if (cloud) {
      const syncAuthNotice = (state) => {
        if (!authWarning) return;
        authWarning.style.display = state.isAuthenticated ? 'none' : 'flex';
      };
      syncAuthNotice(cloud.authState);
      unsubCloudAuth = cloud.subscribe(syncAuthNotice);
      btnAssignAuth?.addEventListener('click', () => {
        cloud.signIn();
      });
    }

    // Auto-select section
    const targetSec = (defaultSection || tmpl.category || 'OPINION').toUpperCase();
    let secFound = false;
    for (let i = 0; i < selectSec.options.length; i++) {
      if (selectSec.options[i].value === targetSec) {
        selectSec.selectedIndex = i;
        secFound = true;
        break;
      }
    }
    if (!secFound && targetSec !== '__CUSTOM__') {
      const newOpt = document.createElement('option');
      newOpt.value = targetSec;
      newOpt.textContent = targetSec;
      newOpt.selected = true;
      selectSec.insertBefore(newOpt, selectSec.lastElementChild);
    }

    if (targetSec === 'PORTADA' || targetSec === 'PRIMERA') {
      inputPage.value = 1;
    } else if (defaultPage) {
      inputPage.value = defaultPage;
    }

    function getSelectedSection() {
      if (selectSec.value === '__custom__') {
        const c = customSec.value.trim().toUpperCase().replace(/[^A-Z0-9_\-]/g, '_');
        return c || 'SECCION';
      }
      return selectSec.value;
    }

    function computeStandardName() {
      const d = inputDate.value || todayStr;
      const p = parseInt(inputPage.value, 10) || 1;
      const padPage = String(p).padStart(2, '0');
      const sec = getSelectedSection();
      const base = `${d}_PAG-${padPage}_${sec}`;
      return versionSuffix ? `${base}${versionSuffix}` : base;
    }

    let checkTimeout = null;

    async function updatePreviewAndCheckCollision() {
      const stem = computeStandardName();
      previewName.textContent = `${stem}.designcraft`;

      clearTimeout(checkTimeout);
      checkTimeout = setTimeout(async () => {
        try {
          const res = await fetch(`/api/documents/exists/${encodeURIComponent(stem)}`);
          if (res.ok) {
            const data = await res.json();
            if (data.exists) {
              collisionBox.style.display = 'block';
              return;
            }
          }
        } catch (e) { }
        collisionBox.style.display = 'none';
      }, 250);
    }

    selectSec.addEventListener('change', () => {
      if (selectSec.value === '__custom__') {
        customSec.style.display = 'block';
        customSec.focus();
      } else {
        customSec.style.display = 'none';
      }
      versionSuffix = '';
      updatePreviewAndCheckCollision();
    });

    customSec.addEventListener('input', () => {
      versionSuffix = '';
      updatePreviewAndCheckCollision();
    });

    inputDate.addEventListener('input', () => {
      versionSuffix = '';
      updatePreviewAndCheckCollision();
    });

    inputPage.addEventListener('input', () => {
      versionSuffix = '';
      updatePreviewAndCheckCollision();
    });

    btnNewVersion.addEventListener('click', () => {
      versionSuffix = '_v2';
      collisionBox.style.display = 'none';
      updatePreviewAndCheckCollision();
    });

    btnOpenExisting.addEventListener('click', () => {
      const stem = computeStandardName();
      const targetUrl = `/?doc=${encodeURIComponent(stem)}&mode=editor`;
      if (window.location.search.includes(`doc=${encodeURIComponent(stem)}`)) {
        window.location.reload();
      } else {
        window.location.href = targetUrl;
      }
    });

    function closeOverlay() {
      if (unsubCloudAuth) unsubCloudAuth();
      overlay.remove();
    }

    btnCancel.addEventListener('click', closeOverlay);
    btnClose.addEventListener('click', closeOverlay);

    btnConfirm.addEventListener('click', async () => {
      const cloud = window.DesignCraftCloud;
      if (cloud && !cloud.authState.isAuthenticated) {
        showCloudToast({
          type: 'error',
          title: 'Google Drive Desconectado',
          message: 'Tu sesión no está activa. Conecta tu cuenta para autorizar la descarga de la maqueta.',
          duration: 6000,
        });
        showGoogleAuthModal({
          title: 'Google Drive Desconectado',
          message: 'Debes iniciar sesión con Google para autorizar la descarga de esta maqueta desde la nube.',
          onLogin: () => cloud.signIn(),
        });
        return;
      }

      const stem = computeStandardName();
      const d = inputDate.value || todayStr;
      const p = parseInt(inputPage.value, 10) || 1;
      const sec = getSelectedSection();

      btnConfirm.disabled = true;
      btnConfirm.textContent = '⏳ Creando maqueta...';

      try {
        await loadTemplateIntoApp(tmpl, stem, d, p, sec, !!versionSuffix);
      } finally {
        btnConfirm.disabled = false;
        btnConfirm.textContent = '✍ Crear y Abrir en Modo Redactor';
      }
    });

    updatePreviewAndCheckCollision();
  }

  // Carga el archivo de plantilla a través del backend hacia la sesión activa con la nomenclatura y metadatos asignados
  async function loadTemplateIntoApp(tmpl, targetDocId, editionDate, pageNum, section, force) {
    const cloud = window.DesignCraftCloud;

    const notif = document.createElement('div');
    notif.style.cssText = 'position:fixed;bottom:24px;left:50%;transform:translateX(-50%);background:#ffffff;color:#0f172a;border:1.5px solid #2563eb;padding:14px 24px;border-radius:12px;box-shadow:0 15px 35px rgba(0,0,0,0.18);z-index:100001;font-family:system-ui;font-size:14px;display:flex;align-items:center;gap:12px;';
    const sizeMb = tmpl.size && tmpl.size > 5 * 1024 * 1024 ? ` (${(tmpl.size / (1024 * 1024)).toFixed(1)} MB)` : '';
    notif.innerHTML = `<span style="font-size:22px;">⏳</span> <span>Preparando página <strong>${targetDocId || tmpl.name}</strong>${sizeMb}...</span>`;
    document.body.appendChild(notif);

    function navigateTo(url) {
      if (targetDocId && window.location.search.includes(`doc=${encodeURIComponent(targetDocId)}`)) {
        window.location.reload();
      } else {
        window.location.href = url;
      }
    }

    try {
      const validToken = await cloud.getValidAccessToken();
      if (!validToken) {
        notif.remove();
        document.getElementById('assign-edition-overlay')?.remove();
        cloud.clearSession();
        showGoogleAuthModal({
          title: 'Google Drive Desconectado',
          message: 'Tu sesión de Google Drive ha expirado o no ha sido iniciada. Conecta tu cuenta de Google para autorizar la descarga de la maqueta.',
          onLogin: () => cloud.signIn(),
        });
        showCloudToast({
          type: 'error',
          title: 'Google Drive Desconectado',
          message: 'Se requiere iniciar sesión con Google para abrir plantillas de la nube.',
          duration: 7000,
        });
        return;
      }

      const openRes = await fetch(`/api/cloud/open/${encodeURIComponent(tmpl.id)}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          accessToken: validToken || cloud.authState.accessToken || '',
          targetDocId: targetDocId || undefined,
          editionDate: editionDate || undefined,
          pageNumber: pageNum || undefined,
          section: section || undefined,
          force: !!force,
        }),
      });

      if (openRes.ok) {
        const data = await openRes.json();
        notif.style.borderColor = '#16a34a';
        notif.innerHTML = `<span style="font-size:22px; color:#16a34a;">✓</span> <span>¡Página lista! Abriendo en modo redactor...</span>`;
        setTimeout(() => {
          navigateTo(data.url || `/?doc=${encodeURIComponent(data.docId)}&mode=editor`);
        }, 500);
        return;
      }

      const errData = await openRes.json().catch(() => ({}));

      if (openRes.status === 401 || errData.error === 'google_auth_error') {
        notif.remove();
        document.getElementById('assign-edition-overlay')?.remove();
        cloud.clearSession();
        showGoogleAuthModal({
          title: 'Sesión de Google Drive Expirada',
          message: errData.message || 'Tu sesión de Google Drive ha expirado. Vuelve a iniciar sesión para autorizar la descarga de la maqueta.',
          onLogin: () => cloud.signIn(),
        });
        showCloudToast({
          type: 'error',
          title: 'Google Drive Desconectado',
          message: 'Tu sesión de Google ha expirado. Reconecta para continuar.',
          duration: 7000,
        });
        return;
      }

      if (openRes.status === 409) {
        const data = errData;
        notif.style.borderColor = '#eab308';
        notif.innerHTML = `<span style="font-size:22px; color:#eab308;">⚠️</span> <span>La página ya existe. Abriendo trabajo existente...</span>`;
        setTimeout(() => {
          navigateTo(data.url || `/?doc=${encodeURIComponent(data.docId)}&mode=editor`);
        }, 800);
        return;
      }

      // Respaldo en caso de fallo de red
      const bytes = await cloud.downloadFileBytes(tmpl.id);
      let cleanStem = targetDocId || tmpl.filename
        .replace(/\.(idml|designcraft)$/i, '')
        .trim()
        .replace(/[^a-zA-Z0-9_\-\u00C0-\u017F ]/g, '_');
      if (!cleanStem) cleanStem = 'plantilla';

      await fetch(`/api/documents/${encodeURIComponent(cleanStem)}`, {
        method: 'POST',
        body: bytes,
      });

      notif.style.borderColor = '#16a34a';
      notif.innerHTML = `<span style="font-size:22px; color:#16a34a;">✓</span> <span>¡Plantilla lista! Abriendo en modo redactor...</span>`;
      setTimeout(() => {
        navigateTo(`/?doc=${encodeURIComponent(cleanStem)}&mode=editor`);
      }, 500);

    } catch (err) {
      notif.remove();
      const isAuthErr = err.message && (err.message.includes('401') || err.message.toLowerCase().includes('sesión') || err.message.toLowerCase().includes('auth') || err.message.toLowerCase().includes('credenciales') || err.message.toLowerCase().includes('google'));
      if (isAuthErr) {
        document.getElementById('assign-edition-overlay')?.remove();
        cloud.clearSession();
        showGoogleAuthModal({
          title: 'Google Drive Desconectado',
          message: err.message || 'Tu sesión ha expirado. Vuelve a conectar tu cuenta para continuar.',
          onLogin: () => cloud.signIn(),
        });
        showCloudToast({
          type: 'error',
          title: 'Google Drive Desconectado',
          message: 'Sesión expirada de Google Drive.',
          duration: 7000,
        });
      } else {
        showCloudToast({
          type: 'error',
          title: 'Error al abrir maqueta',
          message: err.message || 'No se pudo abrir la plantilla.',
          duration: 7000,
        });
      }
    }
  }

  // --- TAB 2: GUARDAR EN LA EDICIÓN DEL DÍA ---
  async function renderSaveTab() {
    const cloud = window.DesignCraftCloud;
    const auth = cloud.authState;

    modalContent.innerHTML = `
      <div class="cloud-user-row">
        <div class="cloud-user-info">
          ${auth.user?.picture ? `<img src="${auth.user.picture}" />` : '👤'}
          <div>
            <div style="font-weight: 600; color: #0f172a; font-size: 14px;">${auth.user?.name || 'Redactor'}</div>
            <div style="font-size: 12px; color: #64748b;">${auth.user?.email || ''}</div>
          </div>
        </div>
        <button class="cloud-btn cloud-btn-ghost" id="btn-cloud-logout" style="padding: 4px 10px; font-size: 12px;">
          Cerrar Sesión
        </button>
      </div>

      <div style="background: #f8fafc; border: 1px solid #e2e8f0; padding: 14px; border-radius: 8px;">
        <h4 style="margin: 0 0 4px 0; font-size: 14px; color: #0f172a; font-weight: 700;">Destino de Publicación en Google Drive</h4>
        <p style="margin: 0; font-size: 12.5px; color: #64748b;">Elige la fecha de la edición y la página donde se depositará el trabajo final.</p>
      </div>

      <div class="cloud-field">
        <label>1. Edición / Fecha del Diario</label>
        <select class="cloud-select" id="select-edicion">
          <option value="">Cargando ediciones...</option>
        </select>
      </div>

      <div class="cloud-field">
        <label>2. Página / Sección Asignada</label>
        <select class="cloud-select" id="select-page" disabled>
          <option value="">Primero selecciona una edición</option>
        </select>
      </div>

      <div class="cloud-field">
        <label>3. Estándar PDF para Prensa / Rotativa</label>
        <select class="cloud-select" id="select-pdf-standard">
          <option value="x1a" selected>PDF/X-1a:2001 (Prensa / Rotativa — Acoplado estricto — Predeterminado)</option>
          <option value="x4">PDF/X-4:2010 (Moderno — Transparencias vivas)</option>
        </select>
        <span style="font-size: 11px; color: #64748b; margin-top: 3px; display: block;">
          PDF/X-1a es el estándar recomendado para periódicos: garantiza 100% CMYK y acopla transparencias para máxima compatibilidad con el RIP.
        </span>
      </div>

      <div id="cloud-action-status" style="display: none;" class="cloud-status-badge"></div>

      <div style="display: flex; gap: 12px; margin-top: 8px;">
        <button class="cloud-btn cloud-btn-success" id="btn-save-page" style="flex: 1;" disabled>
          💾 Guardar Maqueta y PDF en Google Drive
        </button>
      </div>
    `;

    document.getElementById('btn-cloud-logout')?.addEventListener('click', () => {
      cloud.signOut();
    });

    const selectEdicion = document.getElementById('select-edicion');
    const selectPage = document.getElementById('select-page');
    const btnSave = document.getElementById('btn-save-page');
    const statusBox = document.getElementById('cloud-action-status');

    if (cachedEdiciones.length === 0) {
      try {
        cachedEdiciones = await cloud.listEdiciones();
      } catch (err) {
        statusBox.style.display = 'block';
        statusBox.style.color = '#dc2626';
        statusBox.textContent = `Error cargando ediciones: ${err.message}`;
      }
    }

    if (cachedEdiciones.length > 0) {
      selectEdicion.innerHTML = `
        <option value="">-- Elige una Edición --</option>
        ${cachedEdiciones.map(e => `<option value="${e.id}">${e.name}</option>`).join('')}
      `;
      if (selectedEdicionId) {
        selectEdicion.value = selectedEdicionId;
      }
    } else {
      selectEdicion.innerHTML = `<option value="">No se encontraron carpetas de edición</option>`;
    }

    selectEdicion.addEventListener('change', async (e) => {
      selectedEdicionId = e.target.value;
      selectedPageId = '';
      selectPage.disabled = true;
      btnSave.disabled = true;

      if (!selectedEdicionId) {
        selectPage.innerHTML = `<option value="">Primero selecciona una edición</option>`;
        return;
      }

      selectPage.innerHTML = `<option value="">Cargando páginas...</option>`;
      try {
        cachedPages = await cloud.listPages(selectedEdicionId);
        if (cachedPages.length > 0) {
          selectPage.disabled = false;
          selectPage.innerHTML = `
            <option value="">-- Elige una Página --</option>
            ${cachedPages.map(p => `<option value="${p.id}">${p.name}</option>`).join('')}
          `;
        } else {
          selectPage.innerHTML = `<option value="">Esta edición no tiene subcarpetas de página</option>`;
        }
      } catch (err) {
        statusBox.style.display = 'block';
        statusBox.style.color = '#dc2626';
        statusBox.textContent = `Error: ${err.message}`;
      }
    });

    selectPage.addEventListener('change', (e) => {
      selectedPageId = e.target.value;
      btnSave.disabled = !Boolean(selectedPageId);
    });

    btnSave.addEventListener('click', async () => {
      if (!selectedPageId) return;
      btnSave.disabled = true;
      statusBox.style.display = 'block';
      statusBox.style.color = '#1d4ed8';
      statusBox.textContent = '⏳ Exportando documento y preparando subida a Google Drive...';

      try {
        const edicionName = cachedEdiciones.find(e => e.id === selectedEdicionId)?.name || 'edicion';
        const pageName = cachedPages.find(p => p.id === selectedPageId)?.name || 'pagina';
        const docName = `${edicionName}_${pageName}.designcraft`;

        // Obtener el ID activo de la URL
        const params = new URLSearchParams(window.location.search);
        const docId = params.get('doc') || 'magazine_cover';

        // Disparar guardado inmediato en memoria antes de empaquetar
        if (typeof window.designcraftSaveNow === 'function') {
          window.designcraftSaveNow();
        }
        await new Promise(r => setTimeout(r, 600));

        statusBox.textContent = `⏳ Empaquetando maqueta (.designcraft), IDML para InDesign, PDF de prensa y vínculos en Drive...`;

        try {
          const accessToken = await cloud.getValidAccessToken();
          if (!accessToken) {
            throw new Error('No hay sesión activa de Google Drive o el token expiró. Por favor vuelve a conectar Google Drive.');
          }

          const selectPdfStd = document.getElementById('select-pdf-standard');
          const pdfStandard = selectPdfStd ? selectPdfStd.value : 'x1a';

          const uploadRes = await fetch('/api/cloud/upload', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
              docId: docId,
              accessToken: accessToken,
              destinationFolderId: selectedPageId,
              isPageFolder: true,
              baseName: `${edicionName}_${pageName}`,
              date: edicionName,
              exportPdf: true,
              pdfStandard: pdfStandard
            })
          });
          if (uploadRes.ok) {
            const upData = await uploadRes.json();
            if (!upData.fileId) {
              throw new Error(upData.error || 'El servidor no devolvió ID de archivo');
            }
            statusBox.style.color = '#16a34a';
            const linksMsg = upData.linksCount ? ` y ${upData.linksCount} recursos vinculados en Links/` : '';
            statusBox.textContent = `✓ ¡Maqueta, IDML InDesign, PDF de prensa guardados en Google Drive!`;
            showCloudToast({
              type: 'success',
              title: '¡Guardado con éxito en Google Drive!',
              message: `Maqueta (.designcraft), IDML para InDesign y PDF de prensa listos para imprenta${linksMsg} en la carpeta de la página (${edicionName} / ${pageName}).`,
              duration: 7000
            });
            setTimeout(() => closeModal(), 1800);
            return;
          } else {
            const errData = await uploadRes.json().catch(() => ({}));
            const errMsg = errData.error || `Error del servidor (${uploadRes.status})`;
            console.warn('[CloudDrive] Backend package upload error:', errMsg);
            statusBox.textContent = `⚠️ Servidor: ${errMsg}. Intentando subida directa desde el navegador...`;
          }
        } catch (serverErr) {
          console.warn('[CloudDrive] Backend package upload error, falling back to direct client upload:', serverErr);
          statusBox.textContent = `⚠️ ${serverErr.message}. Intentando subida directa desde el navegador...`;
        }

        // Respaldo cliente directo si falla la llamada al backend:
        const docRes = await fetch(`/api/documents/${encodeURIComponent(docId)}`);
        if (!docRes.ok) throw new Error('No se pudo obtener el documento actual guardado');
        const docBytes = new Uint8Array(await docRes.arrayBuffer());

        statusBox.textContent = `⏳ Subiendo maqueta \`${docName}\` a Google Drive...`;
        await cloud.uploadFile(docName, docBytes, selectedPageId, 'application/vnd.designcraft+zip');

        // Subir IDML para Adobe InDesign
        try {
          const idmlRes = await fetch(`/api/documents/${encodeURIComponent(docId)}/idml`);
          if (idmlRes.ok) {
            statusBox.textContent = `⏳ Subiendo IDML para Adobe InDesign...`;
            const idmlBytes = new Uint8Array(await idmlRes.arrayBuffer());
            const idmlName = docName.replace(/\.designcraft$/i, '') + '.idml';
            await cloud.uploadFile(idmlName, idmlBytes, selectedPageId, 'application/vnd.adobe.indesign-idml-package');
          }
        } catch (idmlErr) {
          console.warn('[CloudDrive] IDML upload error:', idmlErr);
        }

        // Subir PDF de prensa para visualización
        try {
          const selectPdfStd = document.getElementById('select-pdf-standard');
          const pdfStandard = selectPdfStd ? selectPdfStd.value : 'x1a';
          const pdfRes = await fetch(`/api/documents/${encodeURIComponent(docId)}/pdf?standard=${encodeURIComponent(pdfStandard)}`);
          if (pdfRes.ok) {
            statusBox.textContent = `⏳ Subiendo PDF de prensa listo para imprenta (${pdfStandard.toUpperCase()})...`;
            const pdfBytes = new Uint8Array(await pdfRes.arrayBuffer());
            await cloud.uploadFile(`${edicionName}_${pageName}_prensa.pdf`, pdfBytes, selectedPageId, 'application/pdf');
          }
        } catch (pdfErr) {
          console.warn('[CloudDrive] PDF upload optional error:', pdfErr);
        }

        statusBox.style.color = '#16a34a';
        statusBox.textContent = `✓ ¡Página maquetada (.designcraft, .idml y PDF) guardados exitosamente en Google Drive!`;
        showCloudToast({
          type: 'success',
          title: '¡Guardado con éxito en Google Drive!',
          message: `Maqueta (.designcraft), IDML para InDesign y PDF guardados en la carpeta de la página (${edicionName} / ${pageName}).`,
          duration: 7000
        });
        setTimeout(() => closeModal(), 1800);
      } catch (err) {
        statusBox.style.color = '#dc2626';
        statusBox.textContent = `❌ Error al guardar en Google Drive: ${err.message}`;
        showCloudToast({
          type: 'error',
          title: 'Error al guardar en Google Drive',
          message: err.message || 'No se pudo completar el guardado en Drive.',
          duration: 8000
        });
      } finally {
        btnSave.disabled = false;
      }
    });
    initEditorialLeaseManager();
  }
}

export function initEditorialLeaseManager() {
  const urlParams = new URLSearchParams(window.location.search);
  const docId = urlParams.get('doc');
  if (!docId) return;

  const mode = urlParams.get('mode') || 'layout';

  // Identity management
  let redactorId = localStorage.getItem('designcraft_redactor_id');
  if (!redactorId) {
    redactorId = 'user-' + Math.random().toString(36).substring(2, 9);
    localStorage.setItem('designcraft_redactor_id', redactorId);
  }

  let redactorName = localStorage.getItem('designcraft_redactor_name');
  if (!redactorName) {
    redactorName = 'Redactor ' + redactorId.slice(-4).toUpperCase();
    localStorage.setItem('designcraft_redactor_name', redactorName);
  }

  let heartbeatTimer = null;
  let leaseAcquired = false;

  // Create badge element
  const badge = document.createElement('div');
  badge.id = 'editorial-lease-badge';
  badge.style.cssText = `
    position: fixed;
    bottom: 14px;
    left: 14px;
    z-index: 9999;
    display: flex;
    align-items: center;
    gap: 8px;
    background: #ffffff;
    border: 1px solid #cbd5e1;
    color: #1e293b;
    padding: 6px 14px;
    border-radius: 20px;
    font-family: system-ui, -apple-system, sans-serif;
    font-size: 12px;
    font-weight: 500;
    box-shadow: 0 4px 14px rgba(0,0,0,0.08);
    transition: all 0.25s ease;
  `;
  badge.innerHTML = `
    <span style="display:inline-block;width:8px;height:8px;border-radius:50%;background:#eab308;animation:pulseDot 1.5s infinite;"></span>
    <span>Verificando concurrencia...</span>
  `;
  document.body.appendChild(badge);

  // Pulse animation style
  if (!document.getElementById('lease-keyframes')) {
    const kf = document.createElement('style');
    kf.id = 'lease-keyframes';
    kf.textContent = `
      @keyframes pulseDot {
        0%, 100% { opacity: 1; transform: scale(1); }
        50% { opacity: 0.4; transform: scale(0.85); }
      }
    `;
    document.head.appendChild(kf);
  }

  async function acquireLease() {
    try {
      const res = await fetch(`/api/documents/${encodeURIComponent(docId)}/lease`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          holder: redactorId,
          holderName: redactorName,
          mode: mode,
        }),
      });

      if (res.ok) {
        leaseAcquired = true;
        badge.innerHTML = `
          <span style="display:inline-block;width:8px;height:8px;border-radius:50%;background:#16a34a;"></span>
          <span>Bloqueo exclusivo: <strong style="color:#0f172a;">${redactorName}</strong></span>
        `;
        badge.title = 'Tienes el control exclusivo para editar esta página sin colisiones.';

        // Setup heartbeat every 30s
        if (!heartbeatTimer) {
          heartbeatTimer = setInterval(async () => {
            try {
              await fetch(`/api/documents/${encodeURIComponent(docId)}/heartbeat`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ holder: redactorId }),
              });
            } catch (err) {
              console.warn('[EditorialLease] Heartbeat ping failed:', err);
            }
          }, 30000);
        }
      } else if (res.status === 423) {
        const data = await res.json().catch(() => ({}));
        showLockedModal(data);
      } else {
        const errData = await res.json().catch(() => ({}));
        console.warn('[EditorialLease] Lease acquire non-200:', errData);
      }
    } catch (e) {
      console.warn('[EditorialLease] Lease check error:', e);
    }
  }

  function showLockedModal(lockedData) {
    badge.innerHTML = `
      <span style="display:inline-block;width:8px;height:8px;border-radius:50%;background:#dc2626;"></span>
      <span>Bloqueado por: <strong style="color:#dc2626;">${lockedData.holderName || 'Otro redactor'}</strong></span>
    `;

    const holder = lockedData.holderName || 'otro redactor';
    const ago = lockedData.secondsAgo ? `hace ${lockedData.secondsAgo} segundos` : 'recientemente';

    const overlay = document.createElement('div');
    overlay.id = 'lease-collision-overlay';
    overlay.style.cssText = `
      position: fixed;
      inset: 0;
      background: rgba(15, 23, 42, 0.65);
      backdrop-filter: blur(6px);
      z-index: 100000;
      display: flex;
      align-items: center;
      justify-content: center;
      font-family: system-ui, -apple-system, sans-serif;
      animation: cloudModalIn 0.2s ease-out;
    `;

    overlay.innerHTML = `
      <div style="background:#ffffff;border-radius:16px;max-width:520px;width:92%;padding:28px;box-shadow:0 25px 60px rgba(0,0,0,0.3);border:1px solid #e2e8f0;">
        <div style="display:flex;align-items:center;gap:14px;margin-bottom:16px;">
          <div style="width:48px;height:48px;border-radius:12px;background:#fef2f2;color:#dc2626;font-size:24px;display:flex;align-items:center;justify-content:center;flex-shrink:0;">
            🔒
          </div>
          <div>
            <h3 style="margin:0;font-size:18px;font-weight:700;color:#0f172a;">Página en Edición Simultánea</h3>
            <p style="margin:2px 0 0;font-size:13px;color:#64748b;">Aviso de concurrencia editorial</p>
          </div>
        </div>

        <div style="background:#fff7ed;border:1px solid #ffedd5;border-radius:10px;padding:14px;margin-bottom:18px;font-size:13.5px;color:#9a3412;line-height:1.5;">
          Esta página está actualmente abierta por <strong>${holder}</strong> (${ago}). Para evitar sobreescribir textos o cambios en caliente, el sistema ha activado la protección contra colisiones.
        </div>

        <div style="display:flex;flex-direction:column;gap:10px;">
          <button id="btn-lease-readonly" style="background:#f1f5f9;color:#334155;border:1px solid #cbd5e1;padding:11px 16px;border-radius:8px;font-size:13.5px;font-weight:600;cursor:pointer;display:flex;align-items:center;justify-content:center;gap:8px;">
            👁️ Continuar en Modo Sólo Lectura
          </button>
          <button id="btn-lease-force" style="background:#dc2626;color:#ffffff;border:none;padding:11px 16px;border-radius:8px;font-size:13.5px;font-weight:600;cursor:pointer;display:flex;align-items:center;justify-content:center;gap:8px;box-shadow:0 4px 12px rgba(220,38,38,0.25);">
            ⚡ Forzar Desbloqueo y Tomar Control
          </button>
          <button id="btn-lease-back" style="background:transparent;color:#64748b;border:none;padding:8px 16px;border-radius:8px;font-size:13px;cursor:pointer;">
            ← Volver a la Galería de Plantillas
          </button>
        </div>
      </div>
    `;

    document.body.appendChild(overlay);

    overlay.querySelector('#btn-lease-readonly').addEventListener('click', () => {
      overlay.remove();
      window.__designcraft_readonly = true;
      badge.innerHTML = `
        <span style="display:inline-block;width:8px;height:8px;border-radius:50%;background:#eab308;"></span>
        <span>Modo Sólo Lectura (Bloqueado por ${holder})</span>
      `;
      showCloudToast({
        type: 'info',
        title: 'Modo Sólo Lectura',
        message: 'Puedes examinar el contenido pero tus cambios no pisarán el trabajo del redactor.',
        duration: 5000,
      });
    });

    overlay.querySelector('#btn-lease-force').addEventListener('click', async () => {
      overlay.remove();
      badge.innerHTML = `
        <span style="display:inline-block;width:8px;height:8px;border-radius:50%;background:#eab308;animation:pulseDot 1.5s infinite;"></span>
        <span>Tomando control forzado...</span>
      `;
      try {
        await fetch(`/api/documents/${encodeURIComponent(docId)}/release`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ force: true }),
        });
        await acquireLease();
        showCloudToast({
          type: 'success',
          title: 'Control Exclusivo Obtenido',
          message: 'Has liberado el bloqueo y tomado el control de edición de la página.',
          duration: 6000,
        });
      } catch (err) {
        console.error('Error forcing lease release:', err);
      }
    });

    overlay.querySelector('#btn-lease-back').addEventListener('click', () => {
      window.location.href = '/galeria';
    });
  }

  // Release on window unload
  window.addEventListener('beforeunload', () => {
    if (leaseAcquired) {
      const releaseUrl = `/api/documents/${encodeURIComponent(docId)}/release`;
      const payload = JSON.stringify({ holder: redactorId });
      if (navigator.sendBeacon) {
        const blob = new Blob([payload], { type: 'application/json' });
        navigator.sendBeacon(releaseUrl, blob);
      } else {
        fetch(releaseUrl, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: payload,
          keepalive: true,
        });
      }
    }
  });

  // Initial acquire
  acquireLease();
}

if (typeof window !== 'undefined') {
  window.addEventListener('DOMContentLoaded', () => {
    initCloudUI();
  });
}
