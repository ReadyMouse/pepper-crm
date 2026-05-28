/*
  Pepper CRM Reconnect Snooze Handler

    Intercepts reconnect/snooze form submits and updates the dashboard without a full page reload.

  INPUT: DOM — .travel-snooze-form elements, [data-snooze-row] rows, stat-*-count elements.
  OUTPUT: POST with Accept: application/json; removes row, decrements stats, shows flash.
  NOTES: Loaded by dashboard.html; server returns JSON when Accept header requests it.

  Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.
*/

(function () {
  function formBody(form) {
    return new URLSearchParams(new FormData(form));
  }

  function parseJsonResponse(res) {
    return res.text().then(function (text) {
      var body;
      try {
        body = text ? JSON.parse(text) : {};
      } catch (_e) {
        throw new Error(
          text.trim() || ("Request failed (" + res.status + ")")
        );
      }
      if (!res.ok || !body.ok) {
        throw new Error(body.error || "Request failed (" + res.status + ")");
      }
      return body;
    });
  }

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

  function updateRandomPickEmptyState() {
    const list = document.getElementById("random-pick-list");
    if (!list) return;
    const hasRows = list.querySelector("[data-snooze-row]");
    if (!hasRows) {
      list.remove();
    }
  }

  function showFlash(message, isError, near) {
    let el = document.getElementById("snooze-flash");
    if (!el) {
      el = document.createElement("p");
      el.id = "snooze-flash";
    }
    el.className = "travel-flash " + (isError ? "travel-flash-err" : "travel-flash-ok");
    el.textContent = message;
    el.hidden = false;
    if (near) {
      near.insertAdjacentElement("afterend", el);
    } else {
      const page = document.querySelector(".page-container");
      if (page && el.parentElement !== page) {
        page.insertBefore(el, page.firstChild);
      }
    }
    window.clearTimeout(el._hideTimer);
    el._hideTimer = window.setTimeout(function () {
      el.hidden = true;
    }, 4000);
  }

  function updateTaskEmptyState() {
    const list = document.getElementById("pending-tasks-list");
    const empty = document.getElementById("pending-tasks-empty");
    if (!list || !empty) return;
    const hasRows = list.querySelector("[data-task-row]");
    empty.hidden = !!hasRows;
  }

  function submitReconnectForm(form) {
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
      body: formBody(form),
    })
      .then(parseJsonResponse)
      .then(function () {
        if (row) {
          showFlash("Reconnect saved — contact removed from this list.", false, row);
          row.remove();
          decrementStat(counter);
          if (counter === "reconnect") {
            updateReconnectEmptyState();
          } else if (counter === "random") {
            updateRandomPickEmptyState();
          }
        } else {
          showFlash("Reconnect saved — contact removed from this list.", false);
        }
      })
      .catch(function (err) {
        showFlash(err.message || "Could not save reconnect setting.", true, row);
      })
      .finally(function () {
        if (btn) {
          btn.disabled = false;
          btn.textContent = prevLabel || "Snooze";
        }
      });
  }

  document.addEventListener("submit", function (e) {
    const form = e.target.closest(".task-complete-form");
    if (!form) return;

    e.preventDefault();

    const row = form.closest("[data-task-row]");
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
      body: formBody(form),
    })
      .then(parseJsonResponse)
      .then(function () {
        if (row) {
          showFlash("Task removed from contact card.", false, row);
          row.remove();
          decrementStat(counter);
          updateTaskEmptyState();
        } else {
          showFlash("Task removed from contact card.", false);
        }
      })
      .catch(function (err) {
        showFlash(err.message || "Could not complete task.", true, row);
      })
      .finally(function () {
        if (btn) {
          btn.disabled = false;
          btn.textContent = prevLabel || "Done";
        }
      });
  });

  document.addEventListener("submit", function (e) {
    const form = e.target.closest(".travel-snooze-form");
    if (!form) return;

    e.preventDefault();
    submitReconnectForm(form);
  });
})();
