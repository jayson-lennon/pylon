(function () {
  var socket = null;

  function log(msg) {
    console.debug('%c[DEVSERVER]: ' + msg, 'background: #42099e;');
  }

  function connect() {
    disconnect();

    const proto = location.protocol.startsWith('https') ? 'wss' : 'ws';
    const wsUri = `${proto}://${location.host}/ws`;

    log('Connecting to dev server...');
    socket = new WebSocket(wsUri);

    socket.onopen = () => {
      log('Connected to dev server');
    }

    socket.onmessage = (ev) => {
      log(`Received message: ${ev.data}`);
      const msg = JSON.parse(ev.data);
      if (msg.type) {
        switch (msg.type) {
          case 'reload':
            log('Reloading page');
            location.reload();
            break;
          case 'notify':
            log(`recv notify msg: ${msg.payload}`);
            const msgEl = document.getElementById("devserver-notify-payload");
            msgEl.innerHTML = msg.payload;
            const msgContainer = document.querySelector(".devserver-notify-container");
            msgContainer.style.opacity = "1.0";
            break;
        }
      } else {
        log(`Error: unhandled message: ${ev.data}`);
      }
    }

    socket.onclose = () => {
      log('Disconnected');
      socket = null;
      setTimeout(() => connect(), 1000);
    }
  }

  function disconnect() {
    if (socket) {
      log('Disconnecting...');
      socket.close();
      socket = null;
    }
  }

  connect();
})();