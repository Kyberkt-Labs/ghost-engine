/// The JavaScript extraction script that walks the live DOM and returns a
/// structured layout tree as a JSON-serialisable object.
///
/// The script is designed to be evaluated via `WebView::evaluate_javascript`.
/// It returns an object shaped like:
/// ```json
/// {
///   "url": "https://...",
///   "title": "Page Title",
///   "nodes": [
///     { "tag": "DIV", "id": "main", "cls": "container",
///       "text": null, "x": 0, "y": 0, "w": 1920, "h": 400,
///       "interactive": true, "role": "button", "children": [1,2] }
///   ]
/// }
/// ```
///
/// Invisible nodes (`display:none`, zero-area, off-screen, `visibility:hidden`,
/// `opacity:0`) are excluded to minimise token cost for LLM consumers.
///
/// **Iframe traversal (TSK-5.2):** When an `<iframe>` element is encountered,
/// the script attempts to access `iframe.contentDocument` (same-origin only).
/// If accessible, the iframe's body is walked recursively and its nodes are
/// merged into the parent tree. The iframe container node gets an `iframeSrc`
/// property. Cross-origin iframes are represented as a single node with
/// `iframeSrc` but no children from inside.
///
/// **Shadow DOM traversal (TSK-5.3):** After walking an element's regular
/// children, the script checks `el.shadowRoot`. If an open shadow root
/// exists, its children are walked and appended to the element's child
/// indices. The element's node gets a `shadowHost: true` marker. Closed
/// shadow roots are inaccessible by design and are skipped.
pub const EXTRACT_LAYOUT_JS: &str = r###"
(function() {
  var nodes = [];

  function isVisible(el) {
    if (!(el instanceof Element)) return false;
    var cs = window.getComputedStyle(el);
    if (cs.display === 'none') return false;
    if (cs.visibility === 'hidden') return false;
    if (parseFloat(cs.opacity) === 0) return false;
    var rect = el.getBoundingClientRect();
    if (rect.width === 0 && rect.height === 0) return false;
    return true;
  }

  function isInteractive(el) {
    var tag = el.tagName;
    if (tag === 'A' || tag === 'BUTTON' || tag === 'SELECT' ||
        tag === 'TEXTAREA') return true;
    if (tag === 'INPUT' && el.type !== 'hidden') return true;
    if (el.hasAttribute('onclick') || el.hasAttribute('tabindex')) return true;
    var role = el.getAttribute('role');
    if (role === 'button' || role === 'link' || role === 'tab' ||
        role === 'menuitem' || role === 'checkbox' || role === 'radio' ||
        role === 'switch' || role === 'textbox' || role === 'combobox') return true;
    if (el.isContentEditable) return true;
    return false;
  }

  function textContent(el) {
    var text = '';
    for (var i = 0; i < el.childNodes.length; i++) {
      var child = el.childNodes[i];
      if (child.nodeType === 3) {
        var t = child.textContent.trim();
        if (t) text += (text ? ' ' : '') + t;
      }
    }
    return text || null;
  }

  function walk(el) {
    if (!isVisible(el)) return -1;
    var rect = el.getBoundingClientRect();
    var childIndices = [];

    // Walk regular children.
    for (var i = 0; i < el.children.length; i++) {
      var ci = walk(el.children[i]);
      if (ci !== -1) childIndices.push(ci);
    }

    // TSK-5.3: Traverse open shadow roots.
    var isShadowHost = false;
    try {
      var sr = el.shadowRoot;
      if (sr) {
        isShadowHost = true;
        var shadowChildren = sr.children || [];
        for (var s = 0; s < shadowChildren.length; s++) {
          var si = walk(shadowChildren[s]);
          if (si !== -1) childIndices.push(si);
        }
      }
    } catch(e) { /* closed shadow root or security restriction */ }

    // TSK-5.2: Traverse same-origin iframe content.
    var iframeSrc = null;
    if (el.tagName === 'IFRAME') {
      iframeSrc = el.src || el.getAttribute('src') || null;
      try {
        var iframeDoc = el.contentDocument;
        if (iframeDoc && iframeDoc.body) {
          var iframeBody = iframeDoc.body;
          for (var f = 0; f < iframeBody.children.length; f++) {
            var fi = walk(iframeBody.children[f]);
            if (fi !== -1) childIndices.push(fi);
          }
        }
      } catch(e) { /* cross-origin — cannot access contentDocument */ }
    }

    var idx = nodes.length;
    var node = {
      tag: el.tagName,
      x: Math.round(rect.x),
      y: Math.round(rect.y),
      w: Math.round(rect.width),
      h: Math.round(rect.height)
    };
    if (el.id) node.id = el.id;
    if (el.className && typeof el.className === 'string' && el.className.trim()) {
      node.cls = el.className.trim();
    }
    var text = textContent(el);
    if (text) node.text = text;
    if (isInteractive(el)) node.interactive = true;
    var role = el.getAttribute('role');
    if (role) node.role = role;
    var ariaLabel = el.getAttribute('aria-label');
    if (ariaLabel) node.ariaLabel = ariaLabel;
    var href = el.getAttribute('href');
    if (href) node.href = href;
    var elType = el.getAttribute('type');
    if (elType) node.type = elType;
    var name = el.getAttribute('name');
    if (name) node.name = name;
    var value = el.value;
    if (typeof value === 'string' && value && el.tagName !== 'OPTION') {
      node.value = value;
    }
    var placeholder = el.getAttribute('placeholder');
    if (placeholder) node.placeholder = placeholder;
    if (iframeSrc) node.iframeSrc = iframeSrc;
    if (isShadowHost) node.shadowHost = true;
    if (childIndices.length > 0) node.children = childIndices;
    nodes.push(node);
    return idx;
  }

  var body = document.body || document.documentElement;
  if (body) walk(body);

  return {
    url: document.URL,
    title: document.title || null,
    nodeCount: nodes.length,
    nodes: nodes
  };
})()
"###;
