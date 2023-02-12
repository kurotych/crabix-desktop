import socket

with open("README.md", "r") as file:
    source_line = "1"
    contents = file.read()
    sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    sock.connect("/tmp/crabix")
    sock.sendall((source_line + " " + contents).encode())

