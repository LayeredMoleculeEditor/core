# LME Core

LME is designed as a C/S pattern application for more flexible usages, and LME core is the server side application.

LME core contains two parts: data manager (DM) module to handle in-memory editor data and RESTful API module for accessing the DM module.

The code can be compiled to a single binary file and could be started with a `--listen` command line argument:

```bash
# start a server on port 12080 of localhost
lme_core --listen 127.0.0.1:12080
```

## Concepts

In LME core, there are three important concepts for handle a molecule model:

- layer: layer is a data structure contains information of atoms and bonds, or a rule to modify the structure got from lower layer.
- stack: when overlaying upper layer upon lower layers, the final structure will generated by the overlay process.
