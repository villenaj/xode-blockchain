nohup ./bin2/eth-rpc --node-rpc-url ws://127.0.0.1:9977 --rpc-port 8546 --prometheus-port 9616 > eth-rpc-xode.log 2>&1 &
nohup ./bin2/eth-rpc --node-rpc-url ws://127.0.0.1:9955 --rpc-port 8547 --prometheus-port 9617 > eth-rpc-assethub.log 2>&1 &
