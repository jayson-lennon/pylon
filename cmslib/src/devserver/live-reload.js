(function () {
  var socket = null;
  var reconnectTimeout = null;

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
      if (ev.data === 'RELOAD') {
        log('Reloading page');
        location.reload();
      } else {
        log(`Error: unhandled message: ${ev.data}`);
      }
    }

    socket.onclose = () => {
      log('Disconnected');
      socket = null;
      reconnectTimeout = setTimeout(() => connect(), 1000);
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