import socket,json,sys
s=socket.socket(socket.AF_UNIX);s.settimeout(12);s.connect('/run/microduck-observer-ota.sock');s.sendall(json.dumps({'op':sys.argv[1]}).encode());s.shutdown(socket.SHUT_WR)
b=b''
while True:
 c=s.recv(8192)
 if not c:break
 b+=c
v=json.loads(b);print(json.dumps(v));assert 'error' not in v
