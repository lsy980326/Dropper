# 외부 구성 요소와 엔진 소스

Dropper는 FFmpeg/ffprobe를 별도 프로세스로 실행합니다. 동봉된 FFmpeg와 x264/x265 구성은 GPL 조건을 따르며, 각 구성 요소의 원래 라이선스·저작권 고지를 유지합니다. 앱 자체 코드의 라이선스는 별도로 지정하지 않았습니다.

## v0.2.0–v0.2.3 대응 소스 다운로드

[v0.2.0 릴리스](https://github.com/lsy980326/Dropper/releases/tag/v0.2.0)에서 소스·패치·빌드 방법·고지·체크섬을 추가 비용 없이 받습니다. 아래 소스 파일은 엔진 검토·재빌드용이며, 앱을 설치하고 사용하는 데는 필요하지 않습니다.

Windows v0.2.1·v0.2.2·v0.2.3의 FFmpeg/ffprobe는 v0.2.0에 포함된 실행 파일과 SHA256이 동일하며 아래 대응 소스를 함께 사용합니다. Windows 빌드는 변경되는 latest URL 대신 동일 바이너리가 들어 있는 고정 배포 아카이브를 검증해 받습니다.

Mac v0.2.3도 v0.2.0의 대응 소스가 제공된 엔진을 재사용합니다. Apple Silicon은 기존 설치 이미지의 엔진·라이브러리·고지를 체크섬으로 확인해 그대로 포함하고, Intel은 고정된 자체 빌드 아카이브를 검증해 재패키징합니다. 앱 전체에는 임시 서명을 적용하며 엔진의 빌드 구성과 대응 소스는 바꾸지 않습니다.

| 설치 파일 | FFmpeg 엔진 | 대응 소스 |
|---|---|---|
| Mac Apple Silicon | 8.0.1 · Homebrew 8.0.1_4 | [macOS 소스 묶음](https://github.com/lsy980326/Dropper/releases/download/v0.2.0/Dropper_0.2.0_macOS_engine_sources.tar.gz) |
| Mac Intel · 재패키징 | 8.0.1 · 자체 정적 빌드 | [macOS 소스 묶음](https://github.com/lsy980326/Dropper/releases/download/v0.2.0/Dropper_0.2.0_macOS_engine_sources.tar.gz) |
| Windows x64 | n8.1.3-35-gf1acb0a266 | [Windows 소스 묶음](https://github.com/lsy980326/Dropper/releases/download/v0.2.0/Dropper_0.2.0_Windows_engine_sources.tar.gz) + [OpenSSL 원본 소스](https://github.com/lsy980326/Dropper/releases/download/v0.2.0/Dropper_0.2.0_Windows_OpenSSL_sources.tar.xz) |

macOS 묶음은 실제 버전의 원본 소스, Homebrew 레시피와 패치, 자체 Intel 빌드 스크립트, 라이선스·저작권 고지를 포함합니다. Intel의 이전 Evermeet 엔진은 제공자의 패치·소스를 충분히 대조할 수 없어 소스가 확인되는 자체 빌드로 교체했습니다.

Windows 소스는 파일당 2GiB 제한 때문에 **기본 묶음과 OpenSSL 원본 소스 두 파일 모두** 받아야 합니다. 기본 묶음의 `README.md`와 `external-source.json`에 배치·검증 방법이 있습니다. 두 파일은 공식 빌드 실행의 소스 캐시 97개, 정확한 FFmpeg 커밋, 제공자 빌드 스크립트·패치, 추가 Rust 의존성 소스와 빌드 중 내려받은 보조 소스를 포함합니다. 원래 고지와 라이선스는 해당 소스에 보존합니다. GCC 런타임 고지와 Runtime Library Exception도 함께 제공합니다.

각 소스 묶음의 `README.md`에 버전·재빌드 방법과 검증 범위를 안내합니다. 소스 묶음 안의 `SHA256SUMS`와 릴리스의 체크섬 파일로 무결성을 확인할 수 있습니다. 설치 파일을 제공하는 동안 대응 소스도 유지하며, 다른 엔진으로 배포할 때는 그 버전에 맞는 소스·고지를 새로 준비해야 합니다.

## 원 프로젝트

- [FFmpeg](https://ffmpeg.org/) · [라이선스 안내](https://ffmpeg.org/legal.html) · [GPL v3 §6(d)](https://www.gnu.org/licenses/gpl-3.0.html#section6)
- [x264](https://www.videolan.org/developers/x264.html) · [x265](https://www.videolan.org/developers/x265.html) · [SVT-AV1](https://gitlab.com/AOMediaCodec/SVT-AV1) · [dav1d](https://code.videolan.org/videolan/dav1d)
- [Windows FFmpeg 빌드 제공자](https://github.com/FFmpeg-Builds/FFmpeg-Builds) · [GCC Runtime Library Exception](https://www.gnu.org/licenses/gcc-exception-3.1.html)
- [Tauri](https://github.com/tauri-apps/tauri) (MIT/Apache-2.0) · [React](https://github.com/facebook/react) (MIT) · [Lucide](https://lucide.dev/license) (ISC)
