(function () {
  function decrementStat(counter) {
    if (!counter) return;
    const el = document.getElementById("stat-" + counter + "-count");
    if (!el) return;
    const n = parseInt(el.textContent, 10);
    if (!Number.isNaN(n) && n > 0) {
      el.textContent = String(n - 1);
    }
  }

  function updateReconnectEmptyState() {
    const list = document.getElementById("reconnect-due-list");
    const empty = document.getElementById("reconnect-due-empty");
    if (!list || !empty) return;
    const hasRows = list.querySelector("[data-snooze-row]");
    empty.hidden = !!hasRows;
  }

  function showFlash(message, isError) {
    let el = document.getElementById("snooze-flash");
    if (!el) {
      el = document.createElement("p");
      el.id = "snooze-flash";
      el.className = "travel-flash";
      const page = document.querySelector(".page-container");
      if (page) {
        page.insertBefore(el, page.firstChild);
      }
    }
    el.className = "travel-flash " + (isError ? "travel-flash-err" : "travel-flash-ok");
    el.textContent = message;
    el.hidden = false;
    window.clearTimeout(el._hideTimer);
    el._hideTimer = window.setTimeout(function () {
      el.hidden = true;
    }, 4000);
  }

  document.addEventListener("submit", function (e) {
    const form = e.target.closest(".travel-snooze-form");
    if (!form) return;

    e.preventDefault();

    const select = form.querySelector('select[name="reconnect"]');
    if (!select || !select.value) {
      select?.focus();
      return;
    }

    const row = form.closest("[data-snooze-row]");
    const counter = row?.dataset.counter;
    const btn = form.querySelector(".btn-snooze");
    const prevLabel = btn?.textContent;

    if (btn) {
      btn.disabled = true;
      btn.textContent = "…";
    }

    fetch(form.action, {
      method: "POST",
      headers: { Accept: "application/json" },
      body: new FormData(form),
    })
      .then(function (res) {
        return res.json().then(function (body) {
          if (!res.ok || !body.ok) {
            throw new Error(body.error || "Snooze failed");
          }
          return body;
        });
      })
      .then(function () {
        if (row) {
          row.remove();
          decrementStat(counter);
          if (counter === "reconnect") {
            updateReconnectEmptyState();
          }
        }
        showFlash("Reconnect interval saved to contact card.", false);
      })
      .catch(function (err) {
        showFlash(err.message || "Could not save snooze.", true);
      })
      .finally(function () {
        if (btn) {
          btn.disabled = false;
          btn.textContent = prevLabel || "Set";
        }
      });
  });
})();
