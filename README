# Turboshell


### keytool
Produce a key with which to sign your code
```
$ tsh keytool -o seedfile
```

Verify that your key is not corrupt and print the Public Key
```
$ tsh keytool seedfile
```

Save your public key
```
$ PUBLIC_KEY=`tsh keytool seedfile`
```

### compile
Produce a signed archive of your code
```
$ tsh compile -d /path/to/code -o archive.tsar -s seedfile $ROLE1 $ROLE2 $ROLE3 etc.
```

### inspect
Extract the tarball from the archive for your debugging pleasure
```
$ tsh inspect -k $PUBLIC_KEY archive.tsar -o archive.tar.gz
```

### run
Run the code from your archive
```
$ tsh run -k $PUBLIC_KEY archive.tsar
```
