(function () {
  var socket = null;

  function log(msg, type = 'status') {
    console.debug(msg);
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
      log('Received: ' + ev.data, 'message');
    }

    socket.onclose = () => {
      log('Disconnected');
      socket = null;
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