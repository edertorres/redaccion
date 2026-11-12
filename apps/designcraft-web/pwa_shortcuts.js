/**
 * DesignCraft Editorial PWA & Keyboard Shortcut Collision Interceptor
 * - Registers PWA Service Worker for desktop app installation
 * - Intercepts collision-prone browser shortcuts (Ctrl+S, Ctrl+D, Ctrl+W, Ctrl+T, Ctrl+F, etc.)
 * - Activates W3C Keyboard Lock API in fullscreen / standalone PWA mode
 * - Prevents accidental tab/window closure with beforeunload
 */

(function initPwaAndShortcuts() {
  // 1. Register PWA Service Worker
  if ('serviceWorker' in navigator) {
    window.addEventListener('load', () => {
      navigator.serviceWorker.register('./sw.js').then((reg) => {
        console.log('[DesignCraft PWA] Service Worker activo:', reg.scope);
      }).catch((err) => {
        console.warn('[DesignCraft PWA] Error al registrar Service Worker:', err);
      });
    });
  }

  // 2. W3C Keyboard Lock API (Locks Ctrl+W, Ctrl+T, Ctrl+N in fullscreen / standalone PWA)
  async function requestKeyboardLock() {
    const isFullscreen = Boolean(document.fullscreenElement);
    const isStandalone = window.matchMedia('(display-mode: standalone)').matches || window.navigator.standalone;
    if ((isFullscreen || isStandalone) && 'keyboard' in navigator && typeof navigator.keyboard.lock === 'function') {
      try {
        await navigator.keyboard.lock(['KeyW', 'KeyN', 'KeyT', 'KeyP', 'KeyS', 'KeyD', 'KeyO', 'KeyF', 'KeyG', 'KeyH', 'KeyJ']);
        console.log('[DesignCraft PWA] Keyboard Lock activado para atajos reservados de InDesign');
      } catch (e) {
        // Ignorar si el navegador restringe el bloqueo de teclas
      }
    }
  }

  document.addEventListener('fullscreenchange', requestKeyboardLock);
  window.addEventListener('load', requestKeyboardLock);

  // 3. Global Capture Phase Shortcut Interceptor
  // Intercepts browser actions in capture phase so the event flows to the canvas and egui
  window.addEventListener('keydown', (e) => {
    const target = e.target;
    const isHtmlInput = target && (
      target.tagName === 'INPUT' ||
      target.tagName === 'TEXTAREA' ||
      target.isContentEditable ||
      (target.classList && (
        target.classList.contains('cloud-input') ||
        target.classList.contains('modal-input') ||
        target.classList.contains('swal2-input')
      ))
    );

    const isMac = navigator.platform.toUpperCase().indexOf('MAC') >= 0;
    const cmdOrCtrl = isMac ? e.metaKey : e.ctrlKey;
    const code = e.code;
    const key = e.key.toLowerCase();

    if (cmdOrCtrl) {
      // Ctrl+S / Cmd+S (Guardar maqueta en DesignCraft)
      if (code === 'KeyS' || key === 's') {
        e.preventDefault();
        return;
      }

      // Ctrl+D / Cmd+D (Colocar / Place imagen o texto en InDesign)
      if (code === 'KeyD' || key === 'd') {
        if (!isHtmlInput) {
          e.preventDefault();
          return;
        }
      }

      // Ctrl+O / Cmd+O (Abrir documento)
      if (code === 'KeyO' || key === 'o') {
        if (!isHtmlInput) {
          e.preventDefault();
          return;
        }
      }

      // Ctrl+P / Cmd+P (Imprimir / Salida de prensa)
      if (code === 'KeyP' || key === 'p') {
        e.preventDefault();
        return;
      }

      // Ctrl+F / Cmd+F (Buscar y Reemplazar)
      if (code === 'KeyF' || key === 'f') {
        if (!isHtmlInput) {
          e.preventDefault();
          return;
        }
      }

      // Ctrl+G / Cmd+G (Agrupar / Desagrupar)
      if (code === 'KeyG' || key === 'g') {
        if (!isHtmlInput) {
          e.preventDefault();
          return;
        }
      }

      // Ctrl+H / Cmd+H (Ocultar/Mostrar guías y bordes vs Historial de navegación)
      if (code === 'KeyH' || key === 'h') {
        if (!isHtmlInput) {
          e.preventDefault();
          return;
        }
      }

      // Ctrl+J / Cmd+J (Ir a página vs Descargas)
      if (code === 'KeyJ' || key === 'j') {
        if (!isHtmlInput) {
          e.preventDefault();
          return;
        }
      }

      // Ctrl+T / Cmd+T (Panel Carácter / Tipografía vs Nueva pestaña)
      if (code === 'KeyT' || key === 't') {
        if (!isHtmlInput) {
          e.preventDefault();
          return;
        }
      }

      // Ctrl+W / Cmd+W (Cerrar documento vs Cerrar pestaña)
      if (code === 'KeyW' || key === 'w') {
        if (!isHtmlInput) {
          e.preventDefault();
          return;
        }
      }
    }

    // Tecla Tab: Al estar sobre el lienzo de maquetación, evita que el navegador mueva el foco fuera del canvas
    if (code === 'Tab' || key === 'tab') {
      if (!isHtmlInput && document.activeElement && document.activeElement.id === 'designcraft_canvas') {
        e.preventDefault();
      }
    }

    // F5 / Ctrl+R: Protección contra recarga accidental si hay cambios pendientes
    if ((code === 'F5' || (cmdOrCtrl && (code === 'KeyR' || key === 'r'))) && !e.shiftKey) {
      if (!isHtmlInput && (window.__designcraft_dirty || window.DesignCraftCloud?.leaseAcquired)) {
        e.preventDefault();
        if (confirm('Tienes cambios pendientes en la edición. ¿Deseas recargar la página?')) {
          window.location.reload();
        }
      }
    }
  }, { capture: true, passive: false });

  // 4. Protección contra cierre de ventana con cambios sin guardar
  window.addEventListener('beforeunload', (e) => {
    if (window.__designcraft_dirty || window.DesignCraftCloud?.leaseAcquired) {
      e.preventDefault();
      e.returnValue = 'Tienes cambios en la maqueta o un bloqueo editorial activo. ¿Seguro que deseas salir?';
      return e.returnValue;
    }
  });

  // 5. Captura de evento de instalación de PWA (beforeinstallprompt)
  window.addEventListener('beforeinstallprompt', (e) => {
    e.preventDefault();
    window.__designcraft_install_prompt = e;
    console.log('[DesignCraft PWA] Disponible para instalación en el escritorio');
  });
})();
