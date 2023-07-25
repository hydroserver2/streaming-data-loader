import argparse
import scheduler
import logging
from logging.handlers import RotatingFileHandler


parser = argparse.ArgumentParser()

parser.add_argument('--service', help='The root URL of the HydroServer service to be used by HydroLoader.')
parser.add_argument('--instance', help='The name of a registered HydroLoader instance.')
parser.add_argument('--username', help='A HydroServer username used to authenticate requests.')
parser.add_argument('--password', help='A HydroServer password used to authenticate requests.')
parser.add_argument('--log', help='The path to the log file for the application.')


if __name__ == "__main__":

    args = parser.parse_args()

    logging.basicConfig(
        format='%(asctime)s %(levelname)-8s %(message)s',
        level=logging.INFO,
        datefmt='%Y-%m-%d %H:%M:%S'
    )

    hydroloader_logger = logging.getLogger('hydroloader')
    scheduler_logger = logging.getLogger('scheduler')

    stream_handler = logging.StreamHandler()
    hydroloader_logger.addHandler(stream_handler)
    scheduler_logger.addHandler(stream_handler)

    if args.log:
        log_handler = RotatingFileHandler(
            filename=args.log,
            mode='a',
            maxBytes=20*1024*1024,
            backupCount=3
        )
        hydroloader_logger.addHandler(log_handler)
        scheduler_logger.addHandler(log_handler)

    if all([
        args.service, args.username, args.password, args.instance
    ]):
        scheduler.HydroLoaderScheduler(
            service=args.service,
            instance=args.instance,
            auth=(args.username, args.password)
        )
        input()
