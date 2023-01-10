# snapio-evtx-extractor
A tool that uses [awssnapio](https://github.com/forensicmatt/aws-snap-io) and [libtsk-rs](https://github.com/forensicmatt/libtsk-rs) to extract EVTX files out of AWS Snapshots.

## Tools
### evtx-extractor
Example tool of layering TSK and AWS Snapshot.

```
.\evtx-extractor.exe -h
A tool that can extract EVTX files from an AWS snapshot. Use the AWS CLI to setup your environment first

Usage: evtx-extractor.exe [OPTIONS] --source <SOURCE> --output <OUTPUT> --disk-cache <DISK_CACHE>

Options:
  -s, --source <SOURCE>          The source to extract EVTX files from. This can be a snapshot or a dd
  -o, --output <OUTPUT>          The output directory to write the EVTX files to
  -d, --disk-cache <DISK_CACHE>  The location to store the disk cache
      --logging <LOGGING>        The logging level to use [default: Info] [possible values: Off, Error, Warn, Info, Debug, Trace]
  -h, --help                     Print help information
  -V, --version                  Print version information
```

## Example:

```
PS D:\Demo> .\evtx-extractor.exe -s snap-0acad277e952dfa05 `
>> --disk-cache .\cache\snap-0acad277e952dfa05 `
>> -o .\output\snap-0acad277e952dfa05 `
>> --logging Info
[2023-01-10 00:46:26][evtx_extractor][INFO] TskVs { handle: 0x20eabb1bf90 }
Extracting Microsoft-Windows-PushNotification-Platform%4Admin.evtx to .\output\snap-0acad277e952dfa05\Microsoft-Windows-PushNotification-Platform%4Admin.evtx
Extracting Amazon EC2Launch.evtx to .\output\snap-0acad277e952dfa05\Amazon EC2Launch.evtx
Extracting Application.evtx to .\output\snap-0acad277e952dfa05\Application.evtx
Extracting HardwareEvents.evtx to .\output\snap-0acad277e952dfa05\HardwareEvents.evtx
Extracting Internet Explorer.evtx to .\output\snap-0acad277e952dfa05\Internet Explorer.evtx
```