include ../Makefile-common.mk

deploy: 
	near deploy i-am-human-staging.testnet ../res/oracle_sbt.wasm
deploy-init:
	near deploy i-am-human-staging.testnet ../res/oracle_sbt.wasm
	near call i-am-human-staging.testnet new '{"authority": "zqMwV9fTRoBOLXwt1mHxBAF3d0Rh9E9xwSAXR3/KL5E=", "metadata": {"spec": "sbt-1.0.0", "name": "Oracle v1", "symbol": "Oracle-SBT"}, "registry": "registry-1.i-am-human.testnet", "claim_ttl": 3600, "admin": "i-am-human-staging.testnet"}' --accountId i-am-human-staging.testnet

