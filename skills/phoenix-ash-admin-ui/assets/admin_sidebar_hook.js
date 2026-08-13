const focusableSelector = [
  "a[href]",
  "button:not([disabled])",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  '[tabindex]:not([tabindex="-1"])',
].join(",")

const AdminSidebar = {
  mounted() {
    this.mobileQuery = window.matchMedia("(max-width: 767px)")

    this.readCollapsed = () => {
      try {
        return window.localStorage.getItem("admin-sidebar-collapsed") === "true"
      } catch (_error) {
        return false
      }
    }

    this.writeCollapsed = (collapsed) => {
      try {
        window.localStorage.setItem("admin-sidebar-collapsed", String(collapsed))
      } catch (_error) {
        // Storage can be unavailable in private or sandboxed contexts.
      }
    }

    this.elements = () => ({
      panel: this.el.querySelector("[data-admin-sidebar-panel]"),
      openButtons: this.el.querySelectorAll('[data-admin-sidebar-action="open"]'),
      collapseButtons: this.el.querySelectorAll('[data-admin-sidebar-action="collapse"]'),
    })

    this.syncVisibility = () => {
      const {panel, openButtons, collapseButtons} = this.elements()
      const hidden = this.mobileQuery.matches && !this.mobileOpen

      if (panel) {
        panel.inert = hidden
        panel.setAttribute("aria-hidden", String(hidden))
      }

      this.el.dataset.mobileOpen = String(this.mobileOpen)
      this.el.dataset.collapsed = String(this.collapsed)
      openButtons.forEach((button) => button.setAttribute("aria-expanded", String(this.mobileOpen)))
      collapseButtons.forEach((button) => button.setAttribute("aria-expanded", String(!this.collapsed)))
    }

    this.open = (trigger) => {
      this.lastTrigger = trigger || document.activeElement
      this.mobileOpen = true
      document.documentElement.classList.add("overflow-hidden")
      this.syncVisibility()

      requestAnimationFrame(() => {
        this.elements().panel?.querySelector(focusableSelector)?.focus()
      })
    }

    this.close = ({restoreFocus = true} = {}) => {
      const wasOpen = this.mobileOpen
      this.mobileOpen = false
      document.documentElement.classList.remove("overflow-hidden")
      this.syncVisibility()

      if (wasOpen && restoreFocus) this.lastTrigger?.focus?.()
    }

    this.toggleCollapse = () => {
      this.collapsed = !this.collapsed
      this.writeCollapsed(this.collapsed)
      this.syncVisibility()
    }

    this.onClick = (event) => {
      const actionButton = event.target.closest("[data-admin-sidebar-action]")

      if (actionButton && this.el.contains(actionButton)) {
        const action = actionButton.dataset.adminSidebarAction
        if (action === "open") this.open(actionButton)
        if (action === "close") this.close()
        if (action === "collapse") this.toggleCollapse()
        return
      }

      if (event.target.closest("[data-admin-sidebar-backdrop]")) {
        this.close()
        return
      }

      const panelLink = event.target.closest("[data-admin-sidebar-panel] a[href]")
      if (panelLink && this.mobileQuery.matches) this.close({restoreFocus: false})
    }

    this.onKeydown = (event) => {
      if (!this.mobileQuery.matches || !this.mobileOpen) return

      if (event.key === "Escape") {
        event.preventDefault()
        this.close()
        return
      }

      if (event.key !== "Tab") return

      const panel = this.elements().panel
      const focusable = [...(panel?.querySelectorAll(focusableSelector) || [])].filter(
        (element) => element.getClientRects().length > 0
      )
      if (focusable.length === 0) return

      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault()
        last.focus()
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault()
        first.focus()
      }
    }

    this.onMediaChange = () => {
      if (!this.mobileQuery.matches) this.close({restoreFocus: false})
      this.syncVisibility()
    }

    this.el.addEventListener("click", this.onClick)
    window.addEventListener("keydown", this.onKeydown)
    this.mobileQuery.addEventListener("change", this.onMediaChange)

    this.mobileOpen = false
    this.collapsed = this.readCollapsed()
    this.syncVisibility()
  },

  updated() {
    this.syncVisibility()
  },

  destroyed() {
    this.el.removeEventListener("click", this.onClick)
    window.removeEventListener("keydown", this.onKeydown)
    this.mobileQuery?.removeEventListener("change", this.onMediaChange)
    document.documentElement.classList.remove("overflow-hidden")
  },
}

export default AdminSidebar
